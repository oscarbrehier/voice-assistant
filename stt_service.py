import sys
from faster_whisper import WhisperModel

class STTService:
	def __init__(self, model="base.en", device="cpu"):
		self.model = WhisperModel(model, device, compute_type="int8")

	def transcribe(self, audio_path):
		try:

			segments, info = self.model.transcribe(
				audio_path, 
				language="en",
				beam_size=5,
				vad_filter=True,
				vad_parameters=dict(
					threshold=0.5,
                    min_speech_duration_ms=250
				))
			
			transcription = " ".join([segment.text for segment in segments])
			return transcription.strip()
		
		except Exception as e:
			print(f"error: {e}", file=sys.stderr)
			return ""
		
	def run(self):
		for line in sys.stdin:
			audio_path = line.strip()

			if not audio_path:
				continue 

			if audio_path == "QUIT":
				break

			result = self.transcribe(audio_path)
			print(result, flush=True)

def main():

	audio_file = sys.argv[1]

	model = WhisperModel("base.en", device="cpu", compute_type="int8")

	segments, info = model.transcribe(audio_file, language="en", beam_size=5)

	transcription = " ".join([segment.text for segment in segments])
	print(transcription.strip())

if __name__ == "__main__":
	service = STTService(model="base.en", device="cpu")
	service.run()