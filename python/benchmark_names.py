import sys
import numpy as np
import time
from faster_whisper import WhisperModel
import sounddevice as sd

MODEL_SIZE = "distil-medium.en"
DEVICE_TYPE = "cuda"
TARGET_DEVICE_ID = 9 

print("--- Available Audio Devices ---")
print(sd.query_devices())
print("-------------------------------\n")

if TARGET_DEVICE_ID is None:
    print("!!! Please set TARGET_DEVICE_ID in the script to your mic's ID.")
    TARGET_DEVICE_ID = sd.default.device[0]

print(f"Loading {MODEL_SIZE}...")
model = WhisperModel(MODEL_SIZE, device=DEVICE_TYPE, compute_type="int8")

def test_name(target_name):
    print(f"\n🎤 Testing: 'Hey {target_name}'")
    print(f"Using Device ID: {TARGET_DEVICE_ID}")
    print("Say it now! (Recording 2.5 seconds...)")

    duration = 2.5
    fs = 16000
    
    recording = sd.rec(
        int(duration * fs), 
        samplerate=fs, 
        channels=1, 
        dtype='float32', 
        device=TARGET_DEVICE_ID
    )
    sd.wait()
    
    segments, _ = model.transcribe(recording.flatten(), beam_size=5)
    
    results = list(segments)
    if not results:
        return "Nothing heard", 0.0
    
    text = results[0].text.strip().lower()
    confidence = results[0].avg_logprob 

    score = min(100, max(0, int((1.5 + confidence) * 66))) 
    
    return text, score

names = ["Scorpion", "Juniper", "Obsidian", "Arcturus", "Apollo"]

for name in names:
    heard, score = test_name(name)
    print(f">> Result: '{heard}'")
    print(f">> Confidence Score: {score}%")
    
    if score > 80:
        print("✅ High Reliability")
    elif score > 50:
        print("⚠️ Moderate Reliability")
    else:
        print("❌ Low Reliability")
        
    time.sleep(1.5)