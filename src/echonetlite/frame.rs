// Echonetlite FRAME
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use crate::echonetlite::EchonetliteEdata;
use bincode;
use core::result;
use std::fmt;

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct EchonetliteFrame<'a> {
    pub ehd: u16,
    pub tid: u16,
    pub seoj: [u8; 3],
    pub deoj: [u8; 3],
    pub esv: u8,
    pub opc: u8,
    pub edata: Vec<EchonetliteEdata<'a>>,
}

impl<'a> EchonetliteFrame<'a> {
    pub fn show(&self) -> String {
        match self.esv {
            // SetI_SNA
            0x50 => format!("SetI_SNAプロパティ値書き込み要求不可応答 N={}", self.opc),
            // SetC_SNA
            0x51 => format!("SetC_SNAプロパティ値書き込み要求不可応答 N={}", self.opc),
            // Get_SNA
            0x52 => format!("Get_SNAプロパティ値読み出し不可応答 N={}", self.opc),
            // INF_SNA
            0x53 => format!("INF_SNAプロパティ値通知不可応答 N={}", self.opc),
            // Set_res
            0x71 => format!("Set_resプロパティ値書き込み応答 N={}", self.opc),
            // Get_res
            0x72 => format!("Get_resプロパティ値読み出し応答 N={}", self.opc),
            // INF
            0x73 => format!("INFプロパティ値通知 N={}", self.opc),
            // INFC
            0x74 => format!("INFCプロパティ値通知(応答要) N={}", self.opc),
            _ => {
                let config = bincode::config::standard()
                    .with_big_endian()
                    .with_fixed_int_encoding();
                let encoded = bincode::encode_to_vec(self, config).unwrap();
                format!(
                    "よくわからないESV値 N={} frame={}",
                    self.opc,
                    encoded
                        .into_iter()
                        .map(|n| format!("{:02X}", n))
                        .collect::<String>()
                )
            }
        }
    }
}

impl<'de, Context> bincode::BorrowDecode<'de, Context> for EchonetliteFrame<'de> {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> core::result::Result<Self, bincode::error::DecodeError> {
        let ehd: u16 = bincode::BorrowDecode::borrow_decode(decoder)?;
        let tid: u16 = bincode::BorrowDecode::borrow_decode(decoder)?;
        let seoj: [u8; 3] = bincode::BorrowDecode::borrow_decode(decoder)?;
        let deoj: [u8; 3] = bincode::BorrowDecode::borrow_decode(decoder)?;
        let esv: u8 = bincode::BorrowDecode::borrow_decode(decoder)?;
        let opc: u8 = bincode::BorrowDecode::borrow_decode(decoder)?;
        let mut edata: Vec<EchonetliteEdata> = Vec::new();
        for _idx in 0..opc {
            edata.push(bincode::BorrowDecode::borrow_decode(decoder)?);
        }
        Ok(Self {
            ehd,
            tid,
            seoj,
            deoj,
            esv,
            opc,
            edata,
        })
    }
}

impl<'a> bincode::Encode for EchonetliteFrame<'a> {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> result::Result<(), bincode::error::EncodeError> {
        bincode::Encode::encode(&self.ehd, encoder)?;
        bincode::Encode::encode(&self.tid, encoder)?;
        bincode::Encode::encode(&self.seoj, encoder)?;
        bincode::Encode::encode(&self.deoj, encoder)?;
        bincode::Encode::encode(&self.esv, encoder)?;
        bincode::Encode::encode(&self.opc, encoder)?;
        for v in &self.edata {
            bincode::Encode::encode(v, encoder)?;
        }
        Ok(())
    }
}

impl<'a> fmt::Display for EchonetliteFrame<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.show())
    }
}

impl<'a> Default for EchonetliteFrame<'a> {
    #[inline]
    fn default() -> Self {
        EchonetliteFrame {
            ehd: 0x1081,
            tid: 0,
            seoj: [0, 0, 0],
            deoj: [0, 0, 0],
            esv: 0,
            opc: 0,
            edata: vec![],
        }
    }
}

#[test]
fn test1() {
    let frame = EchonetliteFrame {
        ehd: 0x1081,
        tid: 0x1234,
        seoj: [0x05, 0xff, 0x01],
        deoj: [0x02, 0x88, 0x01],
        esv: 0x62,
        opc: 0x01,
        edata: vec![EchonetliteEdata {
            epc: 0xe7,
            pdc: 0,
            edt: &[],
        }],
    };

    let binary: Vec<u8> = vec![
        0x10, 0x81, //
        0x12, 0x34, //
        0x05, 0xff, 0x01, //
        0x02, 0x88, 0x01, //
        0x62, //
        0x01, //
        0xe7, 0x00, //
    ];
    let config = bincode::config::standard()
        .with_big_endian()
        .with_fixed_int_encoding();

    let encoded = bincode::encode_to_vec(&frame, config).unwrap();
    assert_eq!(encoded.len(), 14);
    assert_eq!(encoded, binary);

    let (decoded, _len): (EchonetliteFrame, usize) =
        bincode::borrow_decode_from_slice(&encoded[..], config).unwrap();
    assert_eq!(frame, decoded);
}
