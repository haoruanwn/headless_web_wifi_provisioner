use crate::structs::Network;
use anyhow::Result;

/// 将 wpa_supplicant 输出中的 `\xHH` 转义序列反转义回原始字节。
/// 主要用于处理扫描结果中 SSID 字段中的汉字等非 ASCII 字符。
pub(super) fn unescape_wpa_ssid(s: &str) -> Vec<u8> {
    fn hex_val(b: u8) -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(10 + b - b'a'),
            b'A'..=b'F' => Some(10 + b - b'A'),
            _ => None,
        }
    }

    let bs = s.as_bytes();
    let mut out = Vec::with_capacity(bs.len());
    let mut i = 0;
    while i < bs.len() {
        if bs[i] == b'\\' {
            // 处理转义序列
            if i + 1 < bs.len() {
                match bs[i + 1] {
                    b'x' | b'X' => {
                        // 期望后面有两个十六进制字符
                        if i + 3 < bs.len() {
                            let h1 = bs[i + 2];
                            let h2 = bs[i + 3];
                            if let (Some(v1), Some(v2)) = (hex_val(h1), hex_val(h2)) {
                                out.push((v1 << 4) | v2);
                                i += 4;
                                continue;
                            }
                        }
                        // 格式不正确，按字面量保留反斜杠
                        out.push(b'\\');
                        i += 1;
                        continue;
                    }
                    b'\\' => {
                        // 双反斜杠 => 一个反斜杠
                        out.push(b'\\');
                        i += 2;
                        continue;
                    }
                    other => {
                        // 未知的转义序列，保留反斜杠和后面的字符
                        out.push(b'\\');
                        out.push(other);
                        i += 2;
                        continue;
                    }
                }
            } else {
                // 字符串以单个 '\' 结尾
                out.push(b'\\');
                i += 1;
                continue;
            }
        } else {
            out.push(bs[i]);
            i += 1;
        }
    }

    out
}

/// 将 IEEE 802.11 信道号转换为频率（MHz）
/// 支持 2.4 GHz 和 5 GHz 频段
/// 参考：https://en.wikipedia.org/wiki/List_of_WLAN_channels
pub(super) fn channel_to_frequency(channel: u8, hw_mode: &str) -> Option<u32> {
    match hw_mode {
        // 2.4 GHz 频段 (802.11b/g/n)
        "b" | "g" => {
            if (1..=13).contains(&channel) {
                // 公式: 2407 + (5 * channel)
                Some(2407 + (5 * channel as u32))
            } else if channel == 14 {
                // 日本特殊频道
                Some(2484)
            } else {
                None
            }
        }
        // 5 GHz 频段 (802.11a/n/ac)
        "a" => {
            // 5 GHz 信道更复杂，支持 UNII-1 到 UNII-4
            match channel {
                36 => Some(5180),
                40 => Some(5200),
                44 => Some(5220),
                48 => Some(5240),
                52 => Some(5260),
                56 => Some(5280),
                60 => Some(5300),
                64 => Some(5320),
                100 => Some(5500),
                104 => Some(5520),
                108 => Some(5540),
                112 => Some(5560),
                116 => Some(5580),
                120 => Some(5600),
                124 => Some(5620),
                128 => Some(5640),
                132 => Some(5660),
                136 => Some(5680),
                140 => Some(5700),
                144 => Some(5720),
                149 => Some(5745),
                153 => Some(5765),
                157 => Some(5785),
                161 => Some(5805),
                165 => Some(5825),
                _ => None,
            }
        }
        _ => None,
    }
}

/// 解析 SCAN_RESULTS 的输出
/// 格式: bssid / frequency / signal level / flags / ssid
pub(super) fn parse_scan_results(output: &str) -> Result<Vec<Network>> {
    let mut networks = Vec::new();
    for line in output.lines().skip(1) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 5 {
            continue;
        }

        let signal_dbm: i16 = parts[2].parse().unwrap_or(-100);
        let flags = parts[3];

        // wpa_supplicant 对包含非 ASCII 字节的 SSID 会以 `\xHH` 转义序列输出。
        // 这里将其反转义回原始字节，然后尝试用 UTF-8 解码（使用 from_utf8_lossy 保持健壮性）。
        let raw_ssid = parts[4];
        let ssid_bytes = unescape_wpa_ssid(raw_ssid);
        let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();

        if ssid.is_empty() {
            continue;
        }

        let security = if flags.contains("WPA2") {
            "WPA2".to_string()
        } else if flags.contains("WPA") {
            "WPA".to_string()
        } else {
            "Open".to_string()
        };

        let signal_percent = ((signal_dbm.clamp(-100, -50) + 100) * 2) as u8;

        networks.push(Network {
            ssid,
            signal: signal_percent,
            security,
        });
    }
    Ok(networks)
}
