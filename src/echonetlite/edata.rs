// Echonetlite EDATA
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use crate::echonetlite::{smart_electric_energy_meter as SM, superclass};
use bincode::de::read::BorrowReader;
use core::result;
use std::fmt;

#[derive(Clone, Eq, PartialEq, Debug)]
pub struct EchonetliteEdata<'a> {
    pub epc: u8,
    pub pdc: u8,
    pub edt: &'a [u8],
}

impl<'a> EchonetliteEdata<'a> {
    pub fn show(&self, opt_unit: Option<&SM::UnitForCumlativeAmountsPower>) -> String {
        if let Ok(a) = SM::Properties::try_from(self) {
            format!("{}", a.show(opt_unit))
        } else if let Ok(a) = superclass::Properties::try_from(self) {
            format!("{}", a.show())
        } else {
            format!(
                "UNKNOWN EPC:0x{:02X}, EDT:[{}]",
                self.epc,
                self.edt
                    .iter()
                    .map(|x| format!("0x{:02X}", x))
                    .collect::<Vec<String>>()
                    .join(",")
            )
        }
    }
}

impl<'a, 'de: 'a, Context> bincode::BorrowDecode<'de, Context> for EchonetliteEdata<'a> {
    fn borrow_decode<D: bincode::de::BorrowDecoder<'de, Context = Context>>(
        decoder: &mut D,
    ) -> core::result::Result<Self, bincode::error::DecodeError> {
        let epc: u8 = bincode::BorrowDecode::borrow_decode(decoder)?;
        let pdc: u8 = bincode::BorrowDecode::borrow_decode(decoder)?;
        decoder.claim_bytes_read(pdc as usize)?;
        let edt = decoder.borrow_reader().take_bytes(pdc as usize)?;
        Ok(Self { epc, pdc, edt })
    }
}

impl<'a> bincode::Encode for EchonetliteEdata<'a> {
    fn encode<E: bincode::enc::Encoder>(
        &self,
        encoder: &mut E,
    ) -> result::Result<(), bincode::error::EncodeError> {
        bincode::Encode::encode(&self.epc, encoder)?;
        bincode::Encode::encode(&self.pdc, encoder)?;
        for v in self.edt {
            bincode::Encode::encode(v, encoder)?;
        }
        Ok(())
    }
}

impl<'a> fmt::Display for EchonetliteEdata<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.show(None))
    }
}

impl<'a> Default for EchonetliteEdata<'a> {
    #[inline]
    fn default() -> Self {
        EchonetliteEdata {
            epc: 0,
            pdc: 0,
            edt: &[],
        }
    }
}

#[test]
fn test1() {
    let e7 = EchonetliteEdata {
        epc: 0xe7,
        pdc: 4,
        edt: &[1, 2, 3, 4],
    };
    let edata = e7.clone();

    let binary: Vec<u8> = vec![0xe7, 0x04, 0x01, 0x02, 0x03, 0x04];
    let config = bincode::config::standard()
        .with_big_endian()
        .with_fixed_int_encoding();
    let encoded = bincode::encode_to_vec(&edata, config).unwrap();
    assert_eq!(encoded, binary);

    let (decoded, _len): (EchonetliteEdata, usize) =
        bincode::borrow_decode_from_slice(&encoded[..], config).unwrap();

    assert_eq!(e7, decoded);
}
