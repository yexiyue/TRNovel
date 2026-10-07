//! Explicit device construction. Accelerator requests never silently select CPU.
use crate::Device;

pub fn cuda(index: usize) -> anyhow::Result<Device> {
    #[cfg(all(feature = "cuda", any(target_os = "windows", target_os = "linux")))]
    {
        Ok(Device::new_cuda(index)?)
    }
    #[cfg(not(all(feature = "cuda", any(target_os = "windows", target_os = "linux"))))]
    anyhow::bail!("CUDA is not compiled for this platform (device {index})");
}

pub fn metal(index: usize) -> anyhow::Result<Device> {
    #[cfg(all(feature = "metal", target_os = "macos"))]
    {
        Ok(Device::new_metal(index)?)
    }
    #[cfg(not(all(feature = "metal", target_os = "macos")))]
    anyhow::bail!("Metal is not compiled for this platform (device {index})");
}

/// Opportunistic library-level selection; worker Auto uses measured calibration.
pub fn auto_device() -> anyhow::Result<Device> {
    if let Ok(device) = cuda(0) {
        return Ok(device);
    }
    if let Ok(device) = metal(0) {
        return Ok(device);
    }
    Ok(Device::Cpu)
}

/// Explicit accelerator names return initialization errors without CPU fallback.
pub fn parse_device(name: &str) -> anyhow::Result<Device> {
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        "cpu" => Ok(Device::Cpu),
        "auto" => auto_device(),
        "cuda" => cuda(0),
        "metal" => metal(0),
        name if name.starts_with("cuda:") => cuda(name[5..].parse()?),
        _ => anyhow::bail!("unknown device '{name}'; expected auto, cpu, cuda, cuda:N or metal"),
    }
}

/// Human-readable label for a [`Device`].
pub fn device_info(device: &Device) -> String {
    match device {
        Device::Cpu => "CPU".to_string(),
        Device::Cuda(_) => "CUDA".to_string(),
        Device::Metal(_) => "Metal".to_string(),
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn invalid_device_names_are_rejected() {
        assert!(super::parse_device("cudafoo").is_err());
        assert!(super::parse_device("cuda:invalid").is_err());
    }

    #[test]
    fn unavailable_accelerators_never_return_cpu() {
        #[cfg(not(all(feature = "cuda", any(target_os = "windows", target_os = "linux"))))]
        assert!(super::cuda(0).is_err());
        #[cfg(not(all(feature = "metal", target_os = "macos")))]
        assert!(super::metal(0).is_err());
    }
}
