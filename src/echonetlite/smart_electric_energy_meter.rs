// Echonetlite 低圧スマートメータークラス
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use crate::echonetlite::EchonetliteEdata;
use chrono::{NaiveDate, NaiveDateTime};
use rust_decimal::Decimal;
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Properties {
    Coefficient(Coefficient),
    NumberOfEffectiveDigits(NumberOfEffectiveDigits),
    CumlativeAmountsPower(CumlativeAmountsPower),
    UnitForCumlativeAmountsPower(UnitForCumlativeAmountsPower),
    HistoricalCumlativeAmount(HistoricalCumlativeAmount),
    InstantiousPower(InstantiousPower),
    InstantiousCurrent(InstantiousCurrent),
    CumlativeAmountsOfPowerAtFixedTime(CumlativeAmountsOfPowerAtFixedTime),
}

impl<'a> Properties {
    pub fn show(&self, opt_unit: Option<&UnitForCumlativeAmountsPower>) -> String {
        match self {
            Self::Coefficient(a) => format!("{}", a),
            Self::NumberOfEffectiveDigits(a) => format!("{}", a),
            Self::CumlativeAmountsPower(a) => a.show(opt_unit),
            Self::UnitForCumlativeAmountsPower(a) => format!("{}", a),
            Self::HistoricalCumlativeAmount(a) => a.show(opt_unit),
            Self::InstantiousPower(a) => format!("{}", a),
            Self::InstantiousCurrent(a) => format!("{}", a),
            Self::CumlativeAmountsOfPowerAtFixedTime(a) => a.show(opt_unit),
        }
    }
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for Properties {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        if let Ok(a) = Coefficient::try_from(edata.clone()) {
            Ok(Properties::Coefficient(a))
        } else if let Ok(a) = NumberOfEffectiveDigits::try_from(edata.clone()) {
            Ok(Properties::NumberOfEffectiveDigits(a))
        } else if let Ok(a) = CumlativeAmountsPower::try_from(edata.clone()) {
            Ok(Properties::CumlativeAmountsPower(a))
        } else if let Ok(a) = UnitForCumlativeAmountsPower::try_from(edata.clone()) {
            Ok(Properties::UnitForCumlativeAmountsPower(a))
        } else if let Ok(a) = HistoricalCumlativeAmount::try_from(edata.clone()) {
            Ok(Properties::HistoricalCumlativeAmount(a))
        } else if let Ok(a) = InstantiousPower::try_from(edata.clone()) {
            Ok(Properties::InstantiousPower(a))
        } else if let Ok(a) = InstantiousCurrent::try_from(edata.clone()) {
            Ok(Properties::InstantiousCurrent(a))
        } else if let Ok(a) = CumlativeAmountsOfPowerAtFixedTime::try_from(edata.clone()) {
            Ok(Properties::CumlativeAmountsOfPowerAtFixedTime(a))
        } else {
            Err(format!("UNKNOWN EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt))
        }
    }
}

impl fmt::Display for Properties {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.show(None))
    }
}

/// 0xd3 係数
#[derive(Serialize, Deserialize, Clone, Eq, PartialEq, Debug)]
pub struct Coefficient(pub u8);

impl Coefficient {
    pub const EPC: u8 = 0xd3; // 0xd3 係数
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for Coefficient {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        if edata.epc == Self::EPC {
            match edata.edt {
                [a] => Ok(Self(*a)),
                [] => Ok(Self(1u8)), // 値なしは × 1.0
                _ => Err(format!("BAD EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt)),
            }
        } else {
            Err(format!("BAD EPC:0x{:X}", edata.epc))
        }
    }
}

impl fmt::Display for Coefficient {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "係数={}", self.0)
    }
}

/// 0xd7 積算電力量有効桁数
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct NumberOfEffectiveDigits(pub u8);

impl NumberOfEffectiveDigits {
    pub const EPC: u8 = 0xd7; // 0xd7 積算電力量有効桁数
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for NumberOfEffectiveDigits {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        match edata.edt {
            [a] if edata.epc == Self::EPC => Ok(Self(*a)),
            _ => Err(format!("BAD EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt)),
        }
    }
}

impl fmt::Display for NumberOfEffectiveDigits {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "積算電力量有効桁数 {} 桁", self.0)
    }
}

/// 0xe0 積算電力量計測値(正方向計測値)
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct CumlativeAmountsPower(pub u32);

impl CumlativeAmountsPower {
    pub const EPC: u8 = 0xe0; // 0xe0 積算電力量計測値(正方向計測値)

    pub fn kwh(&self, unit: &UnitForCumlativeAmountsPower) -> Decimal {
        return Decimal::from(self.0) * unit.0;
    }

    pub fn show(&self, opt_unit: Option<&UnitForCumlativeAmountsPower>) -> String {
        match opt_unit {
            Some(unit) => format!("積算電力量計測値(正方向計測値)={:8} kwh", self.kwh(unit)),
            None => format!("積算電力量計測値(正方向計測値)={:8}", self.0),
        }
    }
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for CumlativeAmountsPower {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        match edata.edt {
            &[a, b, c, d] if edata.epc == Self::EPC => Ok(Self(u32::from_be_bytes([a, b, c, d]))),
            _ => Err(format!("BAD EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt)),
        }
    }
}

impl fmt::Display for CumlativeAmountsPower {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.show(None))
    }
}

/// 0xe1 積算電力量単位(正方向、逆方向計測値)
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct UnitForCumlativeAmountsPower(pub Decimal);

impl UnitForCumlativeAmountsPower {
    pub const EPC: u8 = 0xe1; // 0xe1 積算電力量単位(正方向、逆方向計測値)
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for UnitForCumlativeAmountsPower {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        match edata.edt {
            [0x00] if edata.epc == Self::EPC => Ok(Self(Decimal::new(1, 0))), // 1.0 kwh
            [0x01] if edata.epc == Self::EPC => Ok(Self(Decimal::new(1, 1))), // 0.1 kwh
            [0x02] if edata.epc == Self::EPC => Ok(Self(Decimal::new(1, 2))), // 0.01 kwh
            [0x03] if edata.epc == Self::EPC => Ok(Self(Decimal::new(1, 3))), // 0.001 kwh
            [0x04] if edata.epc == Self::EPC => Ok(Self(Decimal::new(1, 4))), // 0.0001 kwh
            [0x0a] if edata.epc == Self::EPC => Ok(Self(Decimal::new(10, 0))), // 10 kwh
            [0x0b] if edata.epc == Self::EPC => Ok(Self(Decimal::new(100, 0))), // 100 kwh
            [0x0c] if edata.epc == Self::EPC => Ok(Self(Decimal::new(1000, 0))), // 1000 kwh
            [0x0d] if edata.epc == Self::EPC => Ok(Self(Decimal::new(10000, 0))), // 10000 kwh
            _ => Err(format!("BAD EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt)),
        }
    }
}

impl fmt::Display for UnitForCumlativeAmountsPower {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "積算電力量単位(正方向、逆方向計測値)= {} kwh", self.0)
    }
}

impl Serialize for UnitForCumlativeAmountsPower {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{} kwh", self.0))
    }
}

struct UnitForCumlativeAmountsPowerVisitor;

impl<'de> Visitor<'de> for UnitForCumlativeAmountsPowerVisitor {
    type Value = UnitForCumlativeAmountsPower;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("\"10000 kwh\" or \"1000 kwh\" or \"100 kwh\" or \"10 kwh\" or \"1.0 kwh\" or \"0.1 kwh\" or \"0.01 kwh\" or \"0.001 kwh\" or \"0.0001 kwh\"")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        match s {
            "1.0 kwh" => Ok(UnitForCumlativeAmountsPower(Decimal::new(1, 0))),
            "0.1 kwh" => Ok(UnitForCumlativeAmountsPower(Decimal::new(1, 1))),
            "0.01 kwh" => Ok(UnitForCumlativeAmountsPower(Decimal::new(1, 2))),
            "0.001 kwh" => Ok(UnitForCumlativeAmountsPower(Decimal::new(1, 3))),
            "0.0001 kwh" => Ok(UnitForCumlativeAmountsPower(Decimal::new(1, 4))),
            "10 kwh" => Ok(UnitForCumlativeAmountsPower(Decimal::new(10, 0))),
            "100 kwh" => Ok(UnitForCumlativeAmountsPower(Decimal::new(100, 0))),
            "1000 kwh" => Ok(UnitForCumlativeAmountsPower(Decimal::new(1000, 0))),
            "10000 kwh" => Ok(UnitForCumlativeAmountsPower(Decimal::new(10000, 0))),
            _ => Err(de::Error::invalid_value(de::Unexpected::Str(s), &self)),
        }
    }
}

impl<'de> Deserialize<'de> for UnitForCumlativeAmountsPower {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(UnitForCumlativeAmountsPowerVisitor)
    }
}

/// 0xe2 積算電力量計測値履歴1 (正方向計測値)
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct HistoricalCumlativeAmount {
    pub n_days_ago: u16,
    pub historical: Vec<Option<u32>>,
}

impl HistoricalCumlativeAmount {
    pub const EPC: u8 = 0xe2; // 0xe2 積算電力量計測値履歴1 (正方向計測値)

    pub fn show(&self, opt_unit: Option<&UnitForCumlativeAmountsPower>) -> String {
        match opt_unit {
            Some(unit) => format!(
                "積算電力量計測値履歴1 (正方向計測値)={:2}日前[{}]",
                self.n_days_ago,
                self.historical
                    .iter()
                    .map(|a: &Option<u32>| a.map_or("NA".to_string(), |n| {
                        format!("{} kwh", Decimal::from(n) * unit.0)
                    }))
                    .map(|s| format!("{:>13}", s))
                    .collect::<Vec<String>>()
                    .join(",")
            ),
            None => format!(
                "積算電力量計測値履歴1 (正方向計測値)={:2}日前[{}]",
                self.n_days_ago,
                self.historical
                    .iter()
                    .map(|a: &Option<u32>| a.map_or("NA".to_string(), |n| format!("{}", n)))
                    .map(|s| format!("{:>9}", s))
                    .collect::<Vec<String>>()
                    .join(",")
            ),
        }
    }
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for HistoricalCumlativeAmount {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        match edata.edt {
            [day0, day1, xs @ ..] if edata.epc == Self::EPC => {
                let day = u16::from_be_bytes([*day0, *day1]);
                let mut vs = Vec::new();
                for quadruple in xs.chunks_exact(4) {
                    let dword = quadruple
                        .try_into()
                        .map(|n: [u8; 4]| u32::from_be_bytes(n))
                        .unwrap();
                    //
                    vs.push(if dword == 0xfffffffe {
                        None
                    } else {
                        Some(dword)
                    });
                }
                Ok(Self {
                    n_days_ago: day,
                    historical: vs,
                })
            }
            _ => Err(format!("BAD EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt)),
        }
    }
}

impl fmt::Display for HistoricalCumlativeAmount {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.show(None))
    }
}

/// 0xe7 瞬時電力計測値
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct InstantiousPower(pub Decimal);

impl InstantiousPower {
    pub const EPC: u8 = 0xe7; // 0xe7 瞬時電力計測値
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for InstantiousPower {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        match edata.edt {
            &[a, b, c, d] if edata.epc == Self::EPC => {
                Ok(Self(Decimal::new(
                    i32::from_be_bytes([a, b, c, d]) as i64,
                    0,
                ))) // マイナスの値もある
            }
            _ => Err(format!("BAD EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt)),
        }
    }
}

impl fmt::Display for InstantiousPower {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "瞬時電力={:5} W", self.0)
    }
}

/// 0xe8 瞬時電流計測値
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct InstantiousCurrent {
    pub r: Decimal,
    pub t: Option<Decimal>,
}

impl InstantiousCurrent {
    pub const EPC: u8 = 0xe8; // 0xe8 瞬時電流計測値
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for InstantiousCurrent {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        match edata.edt {
            &[a, b, c, d] if edata.epc == Self::EPC => {
                let rt = match (i16::from_be_bytes([a, b]), i16::from_be_bytes([c, d])) {
                    (r, 0x7ffe) => (Decimal::new(r as i64, 1), None), // 単相2線式
                    (r, t) => (Decimal::new(r as i64, 1), Some(Decimal::new(t as i64, 1))),
                };
                Ok(Self { r: rt.0, t: rt.1 })
            }
            _ => Err(format!("BAD EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt)),
        }
    }
}

impl fmt::Display for InstantiousCurrent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match (self.r, self.t) {
            (r, None) => write!(f, "瞬時電流:(1φ2W) {:4} A", r),
            (r, Some(t)) => write!(f, "瞬時電流:(1φ3W) R={:4} A, T={:4} A", r, t),
        }
    }
}

/// 0xea 定時積算電力量計測値(正方向計測値)
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct CumlativeAmountsOfPowerAtFixedTime {
    pub time_point: NaiveDateTime,
    pub cumlative_amounts_power: u32,
}

impl CumlativeAmountsOfPowerAtFixedTime {
    pub const EPC: u8 = 0xea; // 0xea 定時積算電力量計測値(正方向計測値)

    pub fn show(&self, opt_unit: Option<&UnitForCumlativeAmountsPower>) -> String {
        match opt_unit {
            Some(unit) => format!(
                "定時積算電力量計測値(正方向計測値)={} ({:8} kwh)",
                self.time_point.format("%Y-%m-%d %H:%M:%S").to_string(),
                Decimal::from(self.cumlative_amounts_power) * unit.0
            ),
            None => format!(
                "定時積算電力量計測値(正方向計測値)={} ({:8})",
                self.time_point.format("%Y-%m-%d %H:%M:%S").to_string(),
                self.cumlative_amounts_power
            ),
        }
    }
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for CumlativeAmountsOfPowerAtFixedTime {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        match edata.edt {
            &[
                year0,                // 年 2bytes
                year1,                //
                month,                // 月 bytes
                day,                  // 日 bytes
                hour,                 // 時 bytes
                minute,               // 分 1bytes
                second,               // 秒 1bytes
                cumlative_watt_hour0, // 積算電力量 4bytes
                cumlative_watt_hour1, //
                cumlative_watt_hour2, //
                cumlative_watt_hour3, //
            ] if edata.epc == Self::EPC => {
                let year = u16::from_be_bytes([year0, year1]);
                let datetime = NaiveDate::from_ymd_opt(year as i32, month as u32, day as u32)
                    .and_then(|a| a.and_hms_opt(hour as u32, minute as u32, second as u32))
                    .unwrap();
                let value = u32::from_be_bytes([
                    cumlative_watt_hour0,
                    cumlative_watt_hour1,
                    cumlative_watt_hour2,
                    cumlative_watt_hour3,
                ]);
                Ok(Self {
                    time_point: datetime,
                    cumlative_amounts_power: value,
                })
            }
            _ => Err(format!("BAD EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt)),
        }
    }
}

impl fmt::Display for CumlativeAmountsOfPowerAtFixedTime {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.show(None))
    }
}
