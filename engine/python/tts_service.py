import sys
import asyncio
import traceback
import edge_tts

class TTSService:
	def __init__(self, voice="en-US-AvaMultilingualNeural"):
		self.voice = voice
		self.counter = 0

	def next_output_path(self):
		self.counter += 1
		return f"output_{self.counter}.mp3"

	async def synthesize(self, text):
		output_path = self.next_output_path()
		communicate = edge_tts.Communicate(text, self.voice)

		await communicate.save(output_path)
		return output_path

	async def run(self):
		print("READY", flush=True)
		loop = asyncio.get_event_loop()

		while True:
			line = await loop.run_in_executor(None, sys.stdin.readline)

			if not line:
				break

			line = line.strip()
			if not line:
				continue

			if line == "QUIT":
				break

			if line.startswith("TEXT: "):
				text = line[len("TEXT: "):]

				try:
					output_path = await self.synthesize(text)
					print(f"DONE {output_path}", flush=True)
				except Exception as e:
					print(f"ERROR: {e}", flush=True)
					traceback.print_exc(file=sys.stderr)
			else:
				print(f"ERROR Unknown command: {line}", flush=True)


if __name__ == "__main__":
	try:
		service = TTSService()
		coro = service.run()
		asyncio.run(coro)
	except KeyboardInterrupt:
		pass
	except Exception as e:
		print(f"error: {e}", file=sys.stderr, flush=True)
		traceback.print_exc(file=sys.stderr)