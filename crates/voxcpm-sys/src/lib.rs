//! Safe ownership over a synchronous, thread-confined native model.
use std::{
    ffi::{CStr, CString, c_char, c_void},
    marker::PhantomData,
    path::Path,
    ptr::NonNull,
    rc::Rc,
};

#[repr(C)]
struct Native {
    _opaque: [u8; 0],
}
type Callback = unsafe extern "C" fn(*mut c_void, *const f32, usize, bool) -> bool;
unsafe extern "C" {
    fn trn_vox_available(device: i32) -> bool;
    fn trn_vox_create(base: *const c_char, acoustic: *const c_char, device: i32) -> *mut Native;
    fn trn_vox_destroy(model: *mut Native);
    fn trn_vox_error(model: *const Native) -> *const c_char;
    fn trn_vox_encode(
        model: *mut Native,
        samples: *const f32,
        length: usize,
        rate: i32,
        callback: Callback,
        context: *mut c_void,
    ) -> bool;
    fn trn_vox_generate(
        model: *mut Native,
        text: *const c_char,
        reference: *const f32,
        reference_len: usize,
        reference_rate: i32,
        encoded: bool,
        transcript: *const c_char,
        max_steps: i32,
        callback: Callback,
        context: *mut c_void,
    ) -> i32;
}

#[derive(Clone, Copy)]
pub enum Device {
    Cpu,
    Cuda,
    Metal,
}
impl Device {
    fn id(self) -> i32 {
        match self {
            Self::Cpu => 0,
            Self::Cuda => 1,
            Self::Metal => 2,
        }
    }
    pub fn available(self) -> bool {
        unsafe { trn_vox_available(self.id()) }
    }
}
pub struct Model {
    native: NonNull<Native>,
    _thread_confined: PhantomData<Rc<()>>,
}
pub struct Reference<'a> {
    pub samples: &'a [f32],
    pub sample_rate: u32,
    pub transcript: &'a str,
    pub encoded: bool,
}
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Complete,
    Cancelled,
    Truncated,
}
fn error(native: *const Native) -> String {
    unsafe {
        CStr::from_ptr(trn_vox_error(native))
            .to_string_lossy()
            .into_owned()
    }
}
impl Model {
    pub fn encode_reference(&mut self, samples: &[f32], rate: u32) -> Result<Vec<f32>, String> {
        if samples.is_empty()
            || !samples.iter().all(|s| s.is_finite())
            || !(8000..=192000).contains(&rate)
        {
            return Err("invalid reference audio".into());
        }
        unsafe extern "C" fn collect(
            context: *mut c_void,
            samples: *const f32,
            len: usize,
            _final: bool,
        ) -> bool {
            // The native function owns the source slice until this callback returns.
            let output = unsafe { &mut *context.cast::<Vec<f32>>() };
            output.extend_from_slice(unsafe { std::slice::from_raw_parts(samples, len) });
            true
        }
        let mut features = Vec::new();
        let ok = unsafe {
            trn_vox_encode(
                self.native.as_ptr(),
                samples.as_ptr(),
                samples.len(),
                rate as i32,
                collect,
                (&mut features as *mut Vec<f32>).cast(),
            )
        };
        if !ok {
            return Err(error(self.native.as_ptr()));
        }
        Ok(features)
    }
    pub fn load(base: &Path, acoustic: &Path, device: Device) -> Result<Self, String> {
        let path = |path: &Path| {
            CString::new(path.to_str().ok_or("model paths must be UTF-8")?)
                .map_err(|e| e.to_string())
        };
        let base = path(base)?;
        let acoustic = path(acoustic)?;
        let native = unsafe { trn_vox_create(base.as_ptr(), acoustic.as_ptr(), device.id()) };
        Ok(Self {
            native: NonNull::new(native).ok_or_else(|| error(std::ptr::null()))?,
            _thread_confined: PhantomData,
        })
    }
    pub fn generate<F: FnMut(&[f32]) -> bool>(
        &mut self,
        text: &str,
        reference: Option<Reference<'_>>,
        max_steps: i32,
        emit: F,
    ) -> Result<Outcome, String> {
        if !(1..=1000).contains(&max_steps) {
            return Err("invalid Vox frame limit".into());
        }
        let text = CString::new(text).map_err(|e| e.to_string())?;
        let transcript = CString::new(reference.as_ref().map_or("", |r| r.transcript))
            .map_err(|e| e.to_string())?;
        let (samples, rate, encoded) = reference.map_or((&[][..], 0, false), |r| {
            (r.samples, r.sample_rate, r.encoded)
        });
        if !samples.iter().all(|s| s.is_finite())
            || (encoded && (samples.is_empty() || transcript.as_bytes().is_empty()))
        {
            return Err("invalid reference features or transcript".into());
        }
        struct State<F> {
            emit: F,
            panicked: bool,
        }
        unsafe extern "C" fn callback<F: FnMut(&[f32]) -> bool>(
            context: *mut c_void,
            samples: *const f32,
            len: usize,
            _final: bool,
        ) -> bool {
            // Native invokes this synchronously while the state and PCM block are alive.
            let state = unsafe { &mut *context.cast::<State<F>>() };
            let pcm = if len == 0 {
                &[]
            } else {
                unsafe { std::slice::from_raw_parts(samples, len) }
            };
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (state.emit)(pcm))) {
                Ok(keep_going) => keep_going,
                Err(_) => {
                    state.panicked = true;
                    false
                }
            }
        }
        let mut state = State {
            emit,
            panicked: false,
        };
        let result = unsafe {
            trn_vox_generate(
                self.native.as_ptr(),
                text.as_ptr(),
                samples.as_ptr(),
                samples.len(),
                rate as i32,
                encoded,
                transcript.as_ptr(),
                max_steps,
                callback::<F>,
                (&mut state as *mut State<F>).cast(),
            )
        };
        if state.panicked {
            return Err("PCM callback panicked".into());
        }
        match result {
            0 => Ok(Outcome::Complete),
            1 => Ok(Outcome::Cancelled),
            2 => Ok(Outcome::Truncated),
            _ => Err(error(self.native.as_ptr())),
        }
    }
}
impl Drop for Model {
    fn drop(&mut self) {
        unsafe { trn_vox_destroy(self.native.as_ptr()) }
    }
}
