[文字转音频网站](https://www.text-to-speech.cn/)

用ffmpeg转换为wav格式
```bash
# 转换 “开始配网”
ffmpeg -i "开始配网.mp3" -ar 16000 -ac 1 -c:a pcm_s16le "开始配网.wav"

# 转换 “正在连接” (你新加的)
ffmpeg -i "正在连接.mp3" -ar 16000 -ac 1 -c:a pcm_s16le "正在连接.wav"

# 转换 “连接成功”
ffmpeg -i "连接成功.mp3" -ar 16000 -ac 1 -c:a pcm_s16le "连接成功.wav"

# 转换 “连接失败”
ffmpeg -i "连接失败，请重新尝试.mp3" -ar 16000 -ac 1 -c:a pcm_s16le "连接失败，请重新尝试.wav"
```
