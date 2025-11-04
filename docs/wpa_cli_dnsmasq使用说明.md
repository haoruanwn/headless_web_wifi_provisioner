直接替换掉原有的文件

```
# /etc/hostapd.conf

# 1. 接口和驱动
interface=wlan0
driver=nl80211

# 2. AP 热点名称 (SSID)
ssid=Echo-mate-Setup

# 3. 硬件模式和信道 (g = 2.4GHz)
hw_mode=g
channel=6

# 4. WPA2 安全设置 
#    (WPA3需要 hostapd v2.8+ 和特定驱动支持，WPA2 兼容性最好)
wpa=2
wpa_key_mgmt=WPA-PSK
# 5. 设置Wi-Fi密码，最好8位
wpa_passphrase=12345678
rsn_pairwise=CCMP
```
```bash
# 创建新的 wpa_supplicant AP 配置文件
cat > /etc/wpa_supplicant_ap.conf << EOF
# AP模式的基础网络
network={
    ssid="Echo-mate-Setup"
    mode=2
    key_mgmt=WPA-PSK
    psk="12345678"
    frequency=2437
}
EOF
```
需要在buildroot里开启如下功能，并确保在shell里能检测到

```bash
[root@root root]# which wpa_cli
/usr/sbin/wpa_cli
[root@root root]# which hostapd
/usr/sbin/hostapd
[root@root root]# which dnsmasq
/usr/sbin/dnsmasq
```

