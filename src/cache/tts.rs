use crate::{errors::Result, utils::novel_catch_dir};
use novel_tts::kokoro_tts::Voice;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::File;
use strum::EnumIter;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, EnumIter, Hash, PartialEq, Eq)]
pub enum Voices {
    // 女声 (Female voices)
    #[default]
    Zf001,
    Zf002,
    Zf003,
    Zf004,
    Zf005,
    Zf006,
    Zf007,
    Zf008,
    Zf017,
    Zf018,
    Zf019,
    Zf021,
    Zf022,
    Zf023,
    Zf024,
    Zf026,
    Zf027,
    Zf028,
    Zf032,
    Zf036,
    Zf038,
    Zf039,
    Zf040,
    Zf042,
    Zf043,
    Zf044,
    Zf046,
    Zf047,
    Zf048,
    Zf049,
    Zf051,
    Zf059,
    Zf060,
    Zf067,
    Zf070,
    Zf071,
    Zf072,
    Zf073,
    Zf074,
    Zf075,
    Zf076,
    Zf077,
    Zf078,
    Zf079,
    Zf083,
    Zf084,
    Zf085,
    Zf086,
    Zf087,
    Zf088,
    Zf090,
    Zf092,
    Zf093,
    Zf094,
    Zf099,

    // 男声 (Male voices)
    Zm009,
    Zm010,
    Zm011,
    Zm012,
    Zm013,
    Zm014,
    Zm015,
    Zm016,
    Zm020,
    Zm025,
    Zm029,
    Zm030,
    Zm031,
    Zm033,
    Zm034,
    Zm035,
    Zm037,
    Zm041,
    Zm045,
    Zm050,
    Zm052,
    Zm053,
    Zm054,
    Zm055,
    Zm056,
    Zm057,
    Zm058,
    Zm061,
    Zm062,
    Zm063,
    Zm064,
    Zm065,
    Zm066,
    Zm068,
    Zm069,
    Zm080,
    Zm081,
    Zm082,
    Zm089,
    Zm091,
    Zm095,
    Zm096,
    Zm097,
    Zm098,
    Zm100,

    // 其他 (Others)
    AfMaple,
    AfSol,
    BfVale,
}

// 为Voices实现Display trait，使Zf001这样的枚举值在打印时中间用下划线分割数字
impl fmt::Display for Voices {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let variant_str = match self {
            Voices::Zf001 => "zf_001",
            Voices::Zf002 => "zf_002",
            Voices::Zf003 => "zf_003",
            Voices::Zf004 => "zf_004",
            Voices::Zf005 => "zf_005",
            Voices::Zf006 => "zf_006",
            Voices::Zf007 => "zf_007",
            Voices::Zf008 => "zf_008",
            Voices::Zf017 => "zf_017",
            Voices::Zf018 => "zf_018",
            Voices::Zf019 => "zf_019",
            Voices::Zf021 => "zf_021",
            Voices::Zf022 => "zf_022",
            Voices::Zf023 => "zf_023",
            Voices::Zf024 => "zf_024",
            Voices::Zf026 => "zf_026",
            Voices::Zf027 => "zf_027",
            Voices::Zf028 => "zf_028",
            Voices::Zf032 => "zf_032",
            Voices::Zf036 => "zf_036",
            Voices::Zf038 => "zf_038",
            Voices::Zf039 => "zf_039",
            Voices::Zf040 => "zf_040",
            Voices::Zf042 => "zf_042",
            Voices::Zf043 => "zf_043",
            Voices::Zf044 => "zf_044",
            Voices::Zf046 => "zf_046",
            Voices::Zf047 => "zf_047",
            Voices::Zf048 => "zf_048",
            Voices::Zf049 => "zf_049",
            Voices::Zf051 => "zf_051",
            Voices::Zf059 => "zf_059",
            Voices::Zf060 => "zf_060",
            Voices::Zf067 => "zf_067",
            Voices::Zf070 => "zf_070",
            Voices::Zf071 => "zf_071",
            Voices::Zf072 => "zf_072",
            Voices::Zf073 => "zf_073",
            Voices::Zf074 => "zf_074",
            Voices::Zf075 => "zf_075",
            Voices::Zf076 => "zf_076",
            Voices::Zf077 => "zf_077",
            Voices::Zf078 => "zf_078",
            Voices::Zf079 => "zf_079",
            Voices::Zf083 => "zf_083",
            Voices::Zf084 => "zf_084",
            Voices::Zf085 => "zf_085",
            Voices::Zf086 => "zf_086",
            Voices::Zf087 => "zf_087",
            Voices::Zf088 => "zf_088",
            Voices::Zf090 => "zf_090",
            Voices::Zf092 => "zf_092",
            Voices::Zf093 => "zf_093",
            Voices::Zf094 => "zf_094",
            Voices::Zf099 => "zf_099",
            Voices::Zm009 => "zm_009",
            Voices::Zm010 => "zm_010",
            Voices::Zm011 => "zm_011",
            Voices::Zm012 => "zm_012",
            Voices::Zm013 => "zm_013",
            Voices::Zm014 => "zm_014",
            Voices::Zm015 => "zm_015",
            Voices::Zm016 => "zm_016",
            Voices::Zm020 => "zm_020",
            Voices::Zm025 => "zm_025",
            Voices::Zm029 => "zm_029",
            Voices::Zm030 => "zm_030",
            Voices::Zm031 => "zm_031",
            Voices::Zm033 => "zm_033",
            Voices::Zm034 => "zm_034",
            Voices::Zm035 => "zm_035",
            Voices::Zm037 => "zm_037",
            Voices::Zm041 => "zm_041",
            Voices::Zm045 => "zm_045",
            Voices::Zm050 => "zm_050",
            Voices::Zm052 => "zm_052",
            Voices::Zm053 => "zm_053",
            Voices::Zm054 => "zm_054",
            Voices::Zm055 => "zm_055",
            Voices::Zm056 => "zm_056",
            Voices::Zm057 => "zm_057",
            Voices::Zm058 => "zm_058",
            Voices::Zm061 => "zm_061",
            Voices::Zm062 => "zm_062",
            Voices::Zm063 => "zm_063",
            Voices::Zm064 => "zm_064",
            Voices::Zm065 => "zm_065",
            Voices::Zm066 => "zm_066",
            Voices::Zm068 => "zm_068",
            Voices::Zm069 => "zm_069",
            Voices::Zm080 => "zm_080",
            Voices::Zm081 => "zm_081",
            Voices::Zm082 => "zm_082",
            Voices::Zm089 => "zm_089",
            Voices::Zm091 => "zm_091",
            Voices::Zm095 => "zm_095",
            Voices::Zm096 => "zm_096",
            Voices::Zm097 => "zm_097",
            Voices::Zm098 => "zm_098",
            Voices::Zm100 => "zm_100",
            Voices::AfMaple => "af_maple",
            Voices::AfSol => "af_sol",
            Voices::BfVale => "bf_vale",
        };
        write!(f, "{}", variant_str)
    }
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
