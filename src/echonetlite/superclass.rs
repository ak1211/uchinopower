// Echonetlite クラス
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use crate::echonetlite::EchonetliteEdata;
use std::fmt;

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum Properties {
    GetPropertyMap(GetPropertyMap),
    Manufacturer(Manufacturer),
    NotifyInstances(NotifyInstances),
}

impl<'a> Properties {
    pub fn show(&self) -> String {
        match self {
            Self::GetPropertyMap(a) => format!("{}", a),
            Self::Manufacturer(a) => format!("{}", a),
            Self::NotifyInstances(a) => format!("{}", a),
        }
    }
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for Properties {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        if let Ok(a) = GetPropertyMap::try_from(edata.clone()) {
            Ok(Properties::GetPropertyMap(a))
        } else if let Ok(a) = Manufacturer::try_from(edata.clone()) {
            Ok(Properties::Manufacturer(a))
        } else if let Ok(a) = NotifyInstances::try_from(edata.clone()) {
            Ok(Properties::NotifyInstances(a))
        } else {
            Err(format!("UNKNOWN EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt))
        }
    }
}

impl fmt::Display for Properties {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.show())
    }
}

#[derive(Clone, Eq, PartialEq, Debug)]
pub enum SmartElectricEnergyMeter {}

/// 0x9f Getプロパティマップ
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct GetPropertyMap {
    properties: Vec<u8>,
}

impl GetPropertyMap {
    pub const EPC: u8 = 0x9f; // 0x9f Getプロパティマップ
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for GetPropertyMap {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        match edata.edt {
            [count, props @ ..] if edata.epc == Self::EPC => {
                let mut get_property_map: Vec<u8> = Vec::with_capacity(*count as usize);
                if *count < 16 {
                    // 16個未満はそのまま
                    get_property_map.copy_from_slice(props);
                } else {
                    // 16個以上は表を参照する
                    let table: [[u8; 8]; 16] = [
                        [0x80, 0x90, 0xa0, 0xb0, 0xc0, 0xd0, 0xe0, 0xf0],
                        [0x81, 0x91, 0xa1, 0xb1, 0xc1, 0xd1, 0xe1, 0xf1],
                        [0x82, 0x92, 0xa2, 0xb2, 0xc2, 0xd2, 0xe2, 0xf2],
                        [0x83, 0x93, 0xa3, 0xb3, 0xc3, 0xd3, 0xe3, 0xf3],
                        [0x84, 0x94, 0xa4, 0xb4, 0xc4, 0xd4, 0xe4, 0xf4],
                        [0x85, 0x95, 0xa5, 0xb5, 0xc5, 0xd5, 0xe5, 0xf5],
                        [0x86, 0x96, 0xa6, 0xb6, 0xc6, 0xd6, 0xe6, 0xf6],
                        [0x87, 0x97, 0xa7, 0xb7, 0xc7, 0xd7, 0xe7, 0xf7],
                        [0x88, 0x98, 0xa8, 0xb8, 0xc8, 0xd8, 0xe8, 0xf8],
                        [0x89, 0x99, 0xa9, 0xb9, 0xc9, 0xd9, 0xe9, 0xf9],
                        [0x8a, 0x9a, 0xaa, 0xba, 0xca, 0xda, 0xea, 0xfa],
                        [0x8b, 0x9b, 0xab, 0xbb, 0xcb, 0xdb, 0xeb, 0xfb],
                        [0x8c, 0x9c, 0xac, 0xbc, 0xcc, 0xdc, 0xec, 0xfc],
                        [0x8d, 0x9d, 0xad, 0xbd, 0xcd, 0xdd, 0xed, 0xfd],
                        [0x8e, 0x9e, 0xae, 0xbe, 0xce, 0xde, 0xee, 0xfe],
                        [0x8f, 0x9f, 0xaf, 0xbf, 0xcf, 0xdf, 0xef, 0xff],
                    ];
                    for row in 0..16 {
                        for col in 0..8 {
                            if props[row] & (1 << col) != 0 {
                                get_property_map.push(table[row][col]);
                            }
                        }
                    }
                    get_property_map.sort();
                }
                Ok(GetPropertyMap {
                    properties: get_property_map,
                })
            }
            _ => Err(format!("BAD EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt)),
        }
    }
}

impl fmt::Display for GetPropertyMap {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Getプロパティマップ [{}]",
            self.properties
                .iter()
                .map(|x| format!("0x{:02X}", x))
                .collect::<Vec<String>>()
                .join(",")
        )
    }
}

/// 0x8a 製造者コード
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct Manufacturer(String);

impl Manufacturer {
    pub const EPC: u8 = 0x8a; // 0x8a メーカーコード
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for Manufacturer {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        match edata.edt {
            triple @ [_0, _1, _2] if edata.epc == Self::EPC => {
                let manufacturer = triple
                    .iter()
                    .map(|n| format!("{:02X}", n))
                    .collect::<String>();
                Ok(Manufacturer(manufacturer))
            }
            _ => Err(format!("BAD EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt)),
        }
    }
}

impl fmt::Display for Manufacturer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "製造者コード(hex)={}", self.0)
    }
}

/// 0xd5 インスタンスリスト通知
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct NotifyInstances {
    count: u8,
    instances: Vec<[u8; 3]>,
}

impl NotifyInstances {
    pub const EPC: u8 = 0xd5; // 0xd5 インスタンスリスト通知
}

impl<'a> TryFrom<EchonetliteEdata<'a>> for NotifyInstances {
    type Error = String;

    fn try_from(edata: EchonetliteEdata) -> Result<Self, Self::Error> {
        match edata.edt {
            [count, data @ ..] if edata.epc == Self::EPC => {
                let instances = data
                    .chunks_exact(3) // 3バイトづつ
                    .map(|triple| {
                        triple
                            .try_into()
                            .map_err(|e: std::array::TryFromSliceError| e.to_string())
                    })
                    .collect::<Vec<Result<[u8; 3], Self::Error>>>();
                instances
                    .into_iter()
                    .collect::<Result<Vec<[u8; 3]>, Self::Error>>()
                    .map(|v| NotifyInstances {
                        count: *count,
                        instances: v,
                    })
            }
            _ => Err(format!("BAD EPC:0x{:X} EDT:{:?}", edata.epc, edata.edt)),
        }
    }
}

impl fmt::Display for NotifyInstances {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ss = self
            .instances
            .iter()
            .map(|[a, b, c]| format!("{:02X}{:02X}{:02X}", a, b, c))
            .collect::<Vec<String>>();
        write!(
            f,
            "インスタンスリスト={:2}個 [{}]",
            self.count,
            ss.join(",")
        )
    }
}
