import sys
import asyncio
import edge_tts

async def speak(text):
    voice = "en-US-AvaNeural"
    communicate = edge_tts.Communicate(text, voice)
    await communicate.save("output.mp3")
    
if __name__ == "__main__":
    text = sys.argv[1]
    asyncio.run(speak(text))
		
