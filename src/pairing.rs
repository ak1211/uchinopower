// 接続するスマートメーターをアクティブスキャンで探す
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use crate::connection_settings::ConnectionSettings;
use crate::echonetlite::{
    EchonetliteEdata, EchonetliteFrame, smart_electric_energy_meter as SM, superclass,
};
use crate::skstack;
use crate::skstack::authn;
use anyhow::Context;
use core::time;
use std::io;
use std::net::Ipv6Addr;
use std::thread;

/// 接続するスマートメーターをアクティブスキャンで探す
pub fn pairing(
    port_reader: &mut io::BufReader<dyn io::Read>,
    port_writer: &mut dyn io::Write,
    scan_time: usize,
    credentials: &authn::Credentials,
) -> anyhow::Result<Option<ConnectionSettings>> {
    // アクティブスキャンを実行する
    let found = skstack::active_scan(port_reader, port_writer, scan_time, credentials)?;

    if let Some(epandesc) = found.first() {
        // MACアドレスからIPv6リンクローカルアドレスへ変換する
        // MACアドレスの最初の1バイト下位2bit目を反転して
        // 0xFE80000000000000XXXXXXXXXXXXXXXXのXXをMACアドレスに置き換える
        let sender = Ipv6Addr::from_bits(
            0xFE80_0000_0000_0000u128 << 64 | (epandesc.addr as u128 ^ 0x0200_0000_0000_0000u128),
        );

        // 検出したスマートメーターと接続する
        authn::connect(
            port_reader,
            port_writer,
            &credentials,
            &sender,
            epandesc.channel,
            epandesc.pan_id,
        )?;

        //
        let props: Vec<EchonetliteEdata> = vec![
            EchonetliteEdata {
                epc: SM::UnitForCumlativeAmountsPower::EPC, // 積算電力量単位(正方向、逆方向計測値)
                ..Default::default()
            },
            EchonetliteEdata {
                epc: superclass::GetPropertyMap::EPC, // Getプロパティマップ
                ..Default::default()
            },
            EchonetliteEdata {
                epc: SM::Coefficient::EPC, // 係数(存在しない場合は×1倍)
                ..Default::default()
            },
            EchonetliteEdata {
                epc: SM::NumberOfEffectiveDigits::EPC, // 積算電力量有効桁数
                ..Default::default()
            },
        ];

        //
        let mut unit_for_cumlative_amounts_power: Option<SM::UnitForCumlativeAmountsPower> = None;
        let mut coefficient: Option<SM::Coefficient> = None;
        //
        for edata in props.iter() {
            let frame = EchonetliteFrame {
                ehd: 0x1081,              // 0x1081 = echonet lite
                tid: 1,                   // tid
                seoj: [0x05, 0xff, 0x01], // home controller
                deoj: [0x02, 0x88, 0x01], // smartmeter
                esv: 0x62,                // get要求
                opc: 1,                   // 1つ
                edata: vec![edata.clone()],
            };
            skstack::send_echonetlite(port_writer, &sender, &frame)?;
            thread::sleep(time::Duration::from_secs(5));
            // イベント受信
            'exit: loop {
                match skstack::receive(port_reader) {
                    Ok(r @ skstack::SkRxD::Ok) => {
                        log::trace!("{:?}", r);
                    }
                    Ok(r @ skstack::SkRxD::Fail(_)) => {
                        log::trace!("{:?}", r);
                    }
                    Ok(r @ skstack::SkRxD::Event(_)) => {
                        log::trace!("{:?}", r);
                    }
                    Ok(r @ skstack::SkRxD::Epandesc(_)) => {
                        log::trace!("{:?}", r);
                    }
                    Ok(skstack::SkRxD::Erxudp(erxudp)) => {
                        let config = bincode::config::standard()
                            .with_big_endian()
                            .with_fixed_int_encoding();
                        let (frame, _len): (EchonetliteFrame, usize) =
                            bincode::borrow_decode_from_slice(&erxudp.data, config).unwrap();

                        let mut s = Vec::<String>::new();
                        s.push(format!("{}", frame));
                        for v in frame.edata.iter() {
                            s.push(format!("{}", v));
                        }
                        log::info!("{}", s.join(" "));
                        // 積算電力量単位値を取り出す
                        for edata in frame.edata {
                            match SM::Properties::try_from(edata) {
                                Ok(SM::Properties::UnitForCumlativeAmountsPower(a)) => {
                                    unit_for_cumlative_amounts_power = Some(a);
                                }
                                Ok(SM::Properties::Coefficient(a)) => {
                                    coefficient = Some(a);
                                }
                                _ => {}
                            }
                        }
                        break 'exit;
                    }
                    Err(e) if e.kind() == io::ErrorKind::TimedOut => break 'exit,
                    Err(e) => return Err(e).context("serial port read failed!"),
                }
            }
        }
        // スマートメータの接続情報
        if let (Some(unit), Some(coeff)) = (unit_for_cumlative_amounts_power, coefficient) {
            let connection_settings = ConnectionSettings {
                RouteBId: credentials.id.to_string(),
                RouteBPassword: credentials.password.to_string(),
                Channel: epandesc.channel,
                MacAddress: format!("{:X}", epandesc.addr),
                PanId: epandesc.pan_id,
                Unit: unit,
                Coefficient: coeff,
            };
            return Ok(Some(connection_settings));
        }
    }

    Ok(None)
}
