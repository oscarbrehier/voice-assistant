import sys
from faster_whisper import WhisperModel
import numpy as np
import traceback

class STTService:
    def __init__(self, model="base.en", device="cpu"):
        print(f"loading {model} model...", file=sys.stderr, flush=True)
        self.model = WhisperModel(model, device, compute_type="int8")
        print("model loaded!", file=sys.stderr, flush=True)
    
    def transcribe(self, audio_data):
        try:
            segments, info = self.model.transcribe(
                audio_data, 
                language="en",
                beam_size=5,
                temperature=0.0,
                compression_ratio_threshold=2.4,
                log_prob_threshold=-1.0,
                no_speech_threshold=0.6,
                condition_on_previous_text=False,
            )
            
            transcription_parts = []
            for segment in segments:
                transcription_parts.append(segment.text.strip())
            
            transcription = " ".join(transcription_parts)
            
            return transcription.strip()
        
        except Exception as e:
            print(f"error in transcribe: {e}", file=sys.stderr, flush=True)
            traceback.print_exc(file=sys.stderr)
            return ""
    
    def run(self):
        print("READY", flush=True)
        
        try:
            while True:
                line_bytes = sys.stdin.buffer.readline()
                
                if not line_bytes:
                    break
                
                line = line_bytes.decode('utf-8').strip()
                
                if not line:
                    continue
                
                if line == "QUIT":
                    break
                
                if line.startswith("AUDIO"):
                    try:
                        parts = line.split()
                        if len(parts) != 2:
                            print("", flush=True)
                            continue
                        
                        num_samples = int(parts[1])
                        bytes_to_read = num_samples * 4
                        
                        audio_bytes = b""
                        while len(audio_bytes) < bytes_to_read:
                            remaining = bytes_to_read - len(audio_bytes)
                            chunk = sys.stdin.buffer.read(min(remaining, 8192))
                            
                            if not chunk:
                                break
                            
                            audio_bytes += chunk
                        
                        if len(audio_bytes) != bytes_to_read:
                            print("", flush=True)
                            continue
                        
                        audio = np.frombuffer(audio_bytes, dtype=np.float32)
                        result = self.transcribe(audio)
                        
                        print(result, flush=True)
                    
                    except Exception as e:
                        print(f"error processing audio: {e}", file=sys.stderr, flush=True)
                        traceback.print_exc(file=sys.stderr)
                        print("", flush=True)
        
        except Exception as e:
            print(f"error in loop: {e}", file=sys.stderr, flush=True)
            traceback.print_exc(file=sys.stderr)

if __name__ == "__main__":
    try:
        service = STTService(model="small.en", device="cpu")
        service.run()
    except Exception as e:
        print(f"error: {e}", file=sys.stderr, flush=True)
        traceback.print_exc(file=sys.stderr)