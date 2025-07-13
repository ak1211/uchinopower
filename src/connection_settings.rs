// スマートメータ接続情報
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use crate::echonetlite::smart_electric_energy_meter as SM;
use serde::{Deserialize, Serialize};

/// スマートメータ接続情報
#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct ConnectionSettings {
    pub RouteBId: String,
    pub RouteBPassword: String,
    pub Channel: u8,
    pub MacAddress: String,
    pub PanId: u16,
    pub Unit: SM::UnitForCumlativeAmountsPower,
    pub Coefficient: SM::Coefficient,
}
