// SKSTACK/IPの応答パーサー
// SPDX-License-Identifier: MPL-2.0
// SPDX-FileCopyrightText: 2025 Akihiro Yamamoto <github.com/ak1211>
//
use crate::skstack::{self, SkRxD};
use nom::branch::alt;
use nom::bytes::complete::{tag, take_while_m_n};
use nom::character::complete::{crlf, hex_digit1, space0, space1};
use nom::combinator::{map, map_res, opt};
use nom::multi::{many0, separated_list1};
use nom::{Parser, bytes};
use std::net::Ipv6Addr;

// 8ビット16進数(任意桁)
fn u8_hex_digit(input: &str) -> nom::IResult<&str, u8> {
    map_res(hex_digit1, |hexd| u8::from_str_radix(hexd, 16)).parse(input)
}

// 8ビット16進数(2桁固定)
fn u8_hex_digit2(input: &str) -> nom::IResult<&str, u8> {
    map_res(take_while_m_n(2, 2, |c: char| c.is_ascii_hexdigit()), |s| {
        u8::from_str_radix(s, 16)
    })
    .parse(input)
}

// 16ビット16進数(任意桁)
fn u16_hex_digit(input: &str) -> nom::IResult<&str, u16> {
    map_res(hex_digit1, |hexd| u16::from_str_radix(hexd, 16)).parse(input)
}

// 64ビット16進数(任意桁)
fn u64_hex_digit(input: &str) -> nom::IResult<&str, u64> {
    map_res(hex_digit1, |hexd| u64::from_str_radix(hexd, 16)).parse(input)
}

// FAIL ERxx\r\n
fn rx_fail(input: &str) -> nom::IResult<&str, SkRxD> {
    let parser = (tag("FAIL ER"), u8_hex_digit2, crlf);
    map(parser, |(_tag, code, _crlf)| SkRxD::Fail(code)).parse(input)
}

// OK\r\n
fn rx_ok(input: &str) -> nom::IResult<&str, SkRxD> {
    map((tag("OK"), crlf), |_| SkRxD::Ok).parse(input)
}

// Ipv6アドレス(FE80:0000:0000:0000:0000:0000:0000:0000)
fn ipv6addr(s: &str) -> nom::IResult<&str, Ipv6Addr> {
    let parser = separated_list1(tag(":"), hex_digit1);
    map_res(parser, |xs: Vec<&str>| xs.join(":").parse::<Ipv6Addr>()).parse(s)
}

// EVENT xx FE80:0000:0000:0000:0000:0000:0000:0000 yy zz\r\n
fn rx_event(s: &str) -> nom::IResult<&str, SkRxD> {
    let (s, _) = tag("EVENT").parse(s)?;
    let (s, _) = space1.parse(s)?;
    let (s, code) = map(u8_hex_digit, |n| n).parse(s)?;
    let (s, _) = space1.parse(s)?;
    let (s, sender_address) = ipv6addr.parse(s)?;
    let (s, _) = space0.parse(s)?;
    let (s, param) = opt(map(u8_hex_digit, |n| n)).parse(s)?;
    let (s, _) = crlf.parse(s)?;
    Ok((
        s,
        SkRxD::Event(skstack::Event {
            code: code,
            sender: sender_address,
            param: param,
        }),
    ))
}

// ERXUDP
fn rx_erxudp(s: &str) -> nom::IResult<&str, SkRxD> {
    //
    let (s, _) = tag("ERXUDP").parse(s)?;
    let (s, _) = space1.parse(s)?;
    // 送信元アドレス
    let (s, sender_address) = ipv6addr.parse(s)?;
    let (s, _) = space1.parse(s)?;
    // 送信先アドレス
    let (s, destination_address) = ipv6addr.parse(s)?;
    let (s, _) = space1.parse(s)?;
    // 送信元ポート番号
    let (s, sender_port) = map(u16_hex_digit, |n| n).parse(s)?;
    let (s, _) = space1.parse(s)?;
    // 送信先ポート番号
    let (s, destination_port) = map(u16_hex_digit, |n| n).parse(s)?;
    let (s, _) = space1.parse(s)?;
    // 送信元のMAC層アドレス
    let (s, senderlla) = u64_hex_digit.parse(s)?;
    let (s, _) = space1.parse(s)?;
    // 暗号化あり/なし
    let (s, secured) = map(u8_hex_digit, |n| n).parse(s)?;
    let (s, _) = space1.parse(s)?;
    // 受信したデータの長さ
    let (s, datalen) = map(u16_hex_digit, |n| n).parse(s)?;
    let (s, _) = space1.parse(s)?;
    // 受信データ(テキスト)
    let (s, data) = many0(u8_hex_digit2).parse(s)?;
    //
    let (s, _) = crlf.parse(s)?;

    //
    let erxudp = skstack::Erxudp {
        sender: sender_address,
        destination: destination_address,
        sender_port,
        destination_port: destination_port,
        senderlla: senderlla,
        secured: secured,
        datalen,
        data: data,
    };

    Ok((s, SkRxD::Erxudp(erxudp)))
}

// EPANDESC
fn rx_epandesc(s: &str) -> nom::IResult<&str, SkRxD> {
    // 1行目
    let (s, _) = (tag("EPANDESC"), crlf).parse(s)?;
    // 2行目
    let (s, _) = bytes::streaming::tag("  ").parse(s)?;
    let (s, channel) = map((tag("Channel:"), u64_hex_digit, crlf), |(_, n, _)| n as u8).parse(s)?;
    // 3行目
    let (s, _) = bytes::streaming::tag("  ").parse(s)?;
    let (s, channel_page) = map((tag("Channel Page:"), u64_hex_digit, crlf), |(_, n, _)| {
        n as u8
    })
    .parse(s)?;
    // 4行目
    let (s, _) = bytes::streaming::tag("  ").parse(s)?;
    let (s, pan_id) = map((tag("Pan ID:"), u64_hex_digit, crlf), |(_, n, _)| n as u16).parse(s)?;
    // 5行目
    let (s, _) = bytes::streaming::tag("  ").parse(s)?;
    let (s, (_, mac_address, _)) = (tag("Addr:"), u64_hex_digit, crlf).parse(s)?;
    // 6行目
    let (s, _) = bytes::streaming::tag("  ").parse(s)?;
    let (s, lqi) = map((tag("LQI:"), u64_hex_digit, crlf), |(_, n, _)| n as u8).parse(s)?;
    // 7行目
    let (s, _) = bytes::streaming::tag("  ").parse(s)?;
    let (s, pair_id) = map((tag("PairID:"), u64_hex_digit, crlf), |(_, n, _)| n as u32).parse(s)?;

    //
    let epandesc = skstack::Epandesc {
        channel: channel,
        channel_page: channel_page,
        pan_id: pan_id,
        addr: mac_address,
        lqi: lqi,
        pair_id: pair_id,
    };

    Ok((s, SkRxD::Epandesc(epandesc)))
}

/// 解析する
pub fn parse_rxd(input: &str) -> nom::IResult<&str, SkRxD> {
    alt((
        // 以下のどれか
        map((space0, crlf), |_| SkRxD::Void), // 空行
        rx_ok,                                // OK
        rx_fail,                              // FAIL
        rx_event,                             // EVENT
        rx_epandesc,                          // EPANDESC
        rx_erxudp,                            // ERXUDP
    ))
    .parse(input)
}

#[test]
fn test1() {
    assert_eq!(parse_rxd("\r\n").unwrap(), ("", SkRxD::Void));

    assert_eq!(parse_rxd(" \r\n").unwrap(), ("", SkRxD::Void));

    assert_eq!(parse_rxd("OK\r\n").unwrap(), ("", SkRxD::Ok));

    assert_eq!(parse_rxd("FAIL ER10\r\n").unwrap(), ("", SkRxD::Fail(16)));

    assert_eq!(u64_hex_digit("FF00").unwrap(), ("", 0xff00));
}

#[test]
fn test2() {
    let sender = "FE80:0000:0000:0000:0000:0000:0000:0000";

    assert_eq!(
        parse_rxd(&format!("EVENT 02 {}\r\n", sender)).unwrap(),
        (
            "",
            SkRxD::Event(skstack::Event {
                code: 2,
                sender: sender.parse().unwrap(),
                param: None,
            })
        )
    );

    assert_eq!(
        parse_rxd(&format!("EVENT 21 {} 02\r\n", sender)).unwrap(),
        (
            "",
            SkRxD::Event(skstack::Event {
                code: 33,
                sender: sender.parse().unwrap(),
                param: Some(2),
            })
        )
    );

    assert_eq!(
        parse_rxd(&format!("EVENT 20 {}\r\n", sender)).unwrap(),
        (
            "",
            SkRxD::Event(skstack::Event {
                code: 0x20,
                sender: sender.parse().unwrap(),
                param: None,
            })
        )
    );
}

#[test]
fn test3() {
    let sender: Ipv6Addr = "FE80:0001:0002:0003:0004:0005:0006:0007".parse().unwrap();
    let destination: Ipv6Addr = "FE80:0008:0009:000a:000b:000c:000d:000e".parse().unwrap();
    let senderlla = 0x1234_5678_9abc_0000u64;
    let datalen = 16;
    let data = "000102030405060708090A0B0C0D0E0F";
    let erxudp = format!(
        "ERXUDP {} {} 02CC 02CC {:X} 1 {:02X} {}\r\n",
        sender.segments().map(|n| format!("{:04X}", n)).join(":"),
        destination
            .segments()
            .map(|n| format!("{:04X}", n))
            .join(":"),
        senderlla,
        datalen,
        data
    );

    assert_eq!(
        parse_rxd(&erxudp).unwrap(),
        (
            "",
            SkRxD::Erxudp(skstack::Erxudp {
                sender: sender,
                destination: destination,
                sender_port: 0x02CC,
                destination_port: 0x02CC,
                senderlla: senderlla,
                secured: 1,
                datalen: datalen,
                data: vec!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15),
            })
        )
    );
}

#[test]
fn test4() {
    let epandesc = vec![
        "EPANDESC\r\n",
        "  Channel:3B\r\n",
        "  Channel Page:09\r\n",
        "  Pan ID:ABCD\r\n",
        "  Addr:12345678ABCDABCD\r\n",
        "  LQI:84\r\n",
        "  PairID:1234ABCD\r\n",
    ];

    assert_eq!(
        parse_rxd(&epandesc.concat()).unwrap(),
        (
            "",
            SkRxD::Epandesc(skstack::Epandesc {
                channel: 59,
                channel_page: 9,
                pan_id: 0xABCD,
                addr: 0x1234_5678_ABCD_ABCD,
                lqi: 132,
                pair_id: 0x1234_ABCD,
            })
        ),
    );

    let incomplete = nom::Err::Incomplete(nom::Needed::new(2));
    let epandesc1 = &epandesc.split_at(1).0;
    let epandesc2 = &epandesc.split_at(2).0;
    let epandesc3 = &epandesc.split_at(3).0;
    let epandesc4 = &epandesc.split_at(4).0;
    let epandesc5 = &epandesc.split_at(5).0;
    let epandesc6 = &epandesc.split_at(6).0;
    let epandesc7 = &epandesc.split_at(7).0;
    assert_eq!(parse_rxd(&epandesc1.concat()).unwrap_err(), incomplete);
    assert_eq!(parse_rxd(&epandesc2.concat()).unwrap_err(), incomplete);
    assert_eq!(parse_rxd(&epandesc3.concat()).unwrap_err(), incomplete);
    assert_eq!(parse_rxd(&epandesc4.concat()).unwrap_err(), incomplete);
    assert_eq!(parse_rxd(&epandesc5.concat()).unwrap_err(), incomplete);
    assert_eq!(parse_rxd(&epandesc6.concat()).unwrap_err(), incomplete);
    assert_eq!(
        parse_rxd(&epandesc7.concat()).unwrap(),
        (
            "",
            SkRxD::Epandesc(skstack::Epandesc {
                channel: 59,
                channel_page: 9,
                pan_id: 0xABCD,
                addr: 0x1234_5678_ABCD_ABCD,
                lqi: 132,
                pair_id: 0x1234_ABCD,
            }),
        )
    );
}
