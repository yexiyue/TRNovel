use crate::{errors::Result, utils::novel_catch_dir};
use novel_tts::kokoro_tts::Voice;
use serde::{Deserialize, Serialize};
use std::fs::File;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default)]
pub enum Voices {
    #[default]
    Zm029,
    Zf048,
    Zf008,
    Zm014,
    Zf003,
    Zf047,
    Zm080,
    Zf094,
    Zf046,
    Zm054,
    Zf001,
    Zm062,
    BfVale,
    Zf044,
    Zf005,
    Zf028,
    Zf059,
    Zm030,
    Zf074,
    Zm009,
    Zf004,
    Zf021,
    Zm095,
    Zm041,
    Zf087,
    Zf039,
    Zm031,
    Zf007,
    Zf038,
    Zf092,
    Zm056,
    Zf099,
    Zm010,
    Zm069,
    Zm016,
    Zm068,
    Zf083,
    Zf093,
    Zf006,
    Zf026,
    Zm053,
    Zm064,
    AfSol,
    Zf042,
    Zf084,
    Zf073,
    Zf067,
    Zm025,
    Zm020,
    Zm050,
    Zf070,
    Zf002,
    Zf032,
    Zm091,
    Zm066,
    Zm089,
    Zm034,
    Zm100,
    Zf086,
    Zf040,
    Zm011,
    Zm098,
    Zm015,
    Zf051,
    Zm065,
    Zf076,
    Zf036,
    Zm033,
    Zf018,
    Zf017,
    Zf049,
    AfMaple,
    Zm082,
    Zm057,
    Zf079,
    Zf022,
    Zm063,
    Zf060,
    Zf019,
    Zm097,
    Zm096,
    Zf023,
    Zf027,
    Zf085,
    Zf077,
    Zm035,
    Zf088,
    Zf024,
    Zf072,
    Zm055,
    Zm052,
    Zf071,
    Zm061,
    Zf078,
    Zm013,
    Zm081,
    Zm037,
    Zf090,
    Zf043,
    Zm058,
    Zm012,
    Zm045,
    Zf075,
}

impl From<Voices> for Voice {
    fn from(value: Voices) -> Self {
        match value {
            Voices::Zm029 => Voice::Zm029(1),
            Voices::Zf048 => Voice::Zf048(1),
            Voices::Zf008 => Voice::Zf008(1),
            Voices::Zm014 => Voice::Zm014(1),
            Voices::Zf003 => Voice::Zf003(1),
            Voices::Zf047 => Voice::Zf047(1),
            Voices::Zm080 => Voice::Zm080(1),
            Voices::Zf094 => Voice::Zf094(1),
            Voices::Zf046 => Voice::Zf046(1),
            Voices::Zm054 => Voice::Zm054(1),
            Voices::Zf001 => Voice::Zf001(1),
            Voices::Zm062 => Voice::Zm062(1),
            Voices::BfVale => Voice::BfVale(1),
            Voices::Zf044 => Voice::Zf044(1),
            Voices::Zf005 => Voice::Zf005(1),
            Voices::Zf028 => Voice::Zf028(1),
            Voices::Zf059 => Voice::Zf059(1),
            Voices::Zm030 => Voice::Zm030(1),
            Voices::Zf074 => Voice::Zf074(1),
            Voices::Zm009 => Voice::Zm009(1),
            Voices::Zf004 => Voice::Zf004(1),
            Voices::Zf021 => Voice::Zf021(1),
            Voices::Zm095 => Voice::Zm095(1),
            Voices::Zm041 => Voice::Zm041(1),
            Voices::Zf087 => Voice::Zf087(1),
            Voices::Zf039 => Voice::Zf039(1),
            Voices::Zm031 => Voice::Zm031(1),
            Voices::Zf007 => Voice::Zf007(1),
            Voices::Zf038 => Voice::Zf038(1),
            Voices::Zf092 => Voice::Zf092(1),
            Voices::Zm056 => Voice::Zm056(1),
            Voices::Zf099 => Voice::Zf099(1),
            Voices::Zm010 => Voice::Zm010(1),
            Voices::Zm069 => Voice::Zm069(1),
            Voices::Zm016 => Voice::Zm016(1),
            Voices::Zm068 => Voice::Zm068(1),
            Voices::Zf083 => Voice::Zf083(1),
            Voices::Zf093 => Voice::Zf093(1),
            Voices::Zf006 => Voice::Zf006(1),
            Voices::Zf026 => Voice::Zf026(1),
            Voices::Zm053 => Voice::Zm053(1),
            Voices::Zm064 => Voice::Zm064(1),
            Voices::AfSol => Voice::AfSol(1),
            Voices::Zf042 => Voice::Zf042(1),
            Voices::Zf084 => Voice::Zf084(1),
            Voices::Zf073 => Voice::Zf073(1),
            Voices::Zf067 => Voice::Zf067(1),
            Voices::Zm025 => Voice::Zm025(1),
            Voices::Zm020 => Voice::Zm020(1),
            Voices::Zm050 => Voice::Zm050(1),
            Voices::Zf070 => Voice::Zf070(1),
            Voices::Zf002 => Voice::Zf002(1),
            Voices::Zf032 => Voice::Zf032(1),
            Voices::Zm091 => Voice::Zm091(1),
            Voices::Zm066 => Voice::Zm066(1),
            Voices::Zm089 => Voice::Zm089(1),
            Voices::Zm034 => Voice::Zm034(1),
            Voices::Zm100 => Voice::Zm100(1),
            Voices::Zf086 => Voice::Zf086(1),
            Voices::Zf040 => Voice::Zf040(1),
            Voices::Zm011 => Voice::Zm011(1),
            Voices::Zm098 => Voice::Zm098(1),
            Voices::Zm015 => Voice::Zm015(1),
            Voices::Zf051 => Voice::Zf051(1),
            Voices::Zm065 => Voice::Zm065(1),
            Voices::Zf076 => Voice::Zf076(1),
            Voices::Zf036 => Voice::Zf036(1),
            Voices::Zm033 => Voice::Zm033(1),
            Voices::Zf018 => Voice::Zf018(1),
            Voices::Zf017 => Voice::Zf017(1),
            Voices::Zf049 => Voice::Zf049(1),
            Voices::AfMaple => Voice::AfMaple(1),
            Voices::Zm082 => Voice::Zm082(1),
            Voices::Zm057 => Voice::Zm057(1),
            Voices::Zf079 => Voice::Zf079(1),
            Voices::Zf022 => Voice::Zf022(1),
            Voices::Zm063 => Voice::Zm063(1),
            Voices::Zf060 => Voice::Zf060(1),
            Voices::Zf019 => Voice::Zf019(1),
            Voices::Zm097 => Voice::Zm097(1),
            Voices::Zm096 => Voice::Zm096(1),
            Voices::Zf023 => Voice::Zf023(1),
            Voices::Zf027 => Voice::Zf027(1),
            Voices::Zf085 => Voice::Zf085(1),
            Voices::Zf077 => Voice::Zf077(1),
            Voices::Zm035 => Voice::Zm035(1),
            Voices::Zf088 => Voice::Zf088(1),
            Voices::Zf024 => Voice::Zf024(1),
            Voices::Zf072 => Voice::Zf072(1),
            Voices::Zm055 => Voice::Zm055(1),
            Voices::Zm052 => Voice::Zm052(1),
            Voices::Zf071 => Voice::Zf071(1),
            Voices::Zm061 => Voice::Zm061(1),
            Voices::Zf078 => Voice::Zf078(1),
            Voices::Zm013 => Voice::Zm013(1),
            Voices::Zm081 => Voice::Zm081(1),
            Voices::Zm037 => Voice::Zm037(1),
            Voices::Zf090 => Voice::Zf090(1),
            Voices::Zf043 => Voice::Zf043(1),
            Voices::Zm058 => Voice::Zm058(1),
            Voices::Zm012 => Voice::Zm012(1),
            Voices::Zm045 => Voice::Zm045(1),
            Voices::Zf075 => Voice::Zf075(1),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TTSConfig {
    pub volume: f32,
    pub speed: f32,
    pub voice: Voices,
    pub auto_play: bool,
}

impl Default for TTSConfig {
    fn default() -> Self {
        Self {
            volume: 1.0,
            speed: 1.0,
            voice: Voices::default(),
            auto_play: false,
        }
    }
}

impl TTSConfig {
    pub fn get_voice(&self) -> Voice {
        self.voice.into()
    }

    pub fn get_cache_file_path() -> Result<std::path::PathBuf> {
        Ok(novel_catch_dir()?.join("tts_config.json"))
    }

    pub fn load() -> Result<Self> {
        match File::open(Self::get_cache_file_path()?) {
            Ok(file) => Ok(serde_json::from_reader(file)?),
            Err(_) => Ok(Self::default()),
        }
    }

    pub fn save(&self) -> Result<()> {
        let file = File::create(Self::get_cache_file_path()?)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn increase_speed(&mut self) {
        self.speed = ((self.speed + 0.1).min(2.0) * 10.0).round() / 10.0;
    }

    pub fn decrease_speed(&mut self) {
        self.speed = ((self.speed - 0.1).max(0.5) * 10.0).round() / 10.0;
    }

    pub fn increase_volume(&mut self) {
        self.volume = ((self.volume + 0.1).min(10.0) * 10.0).round() / 10.0;
    }

    pub fn decrease_volume(&mut self) {
        self.volume = ((self.volume - 0.1).max(0.0) * 10.0).round() / 10.0;
    }
}

impl Drop for TTSConfig {
    fn drop(&mut self) {
        let _ = self.save();
    }
}
