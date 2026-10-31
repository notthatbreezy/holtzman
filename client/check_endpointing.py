"""Replay 16 kHz WAVs with reference speech-end times. Never opens audio devices.

Manifest: [{wav, speech_end_ms, text?}]. Optionally --transcription-url compares
Whisper text on each cut with the complete recording. No game tools are called.
"""
import argparse
import asyncio
import json
import time
import wave
from pathlib import Path
from client.audio import Segmenter,wav_bytes
from client.endpointing import worker
from client.state import load_settings

async def main(args):
    results=[]
    for case in json.loads(Path(args.manifest).read_text(encoding="utf-8")):
        with wave.open(case["wav"],"rb") as source:
            if (source.getnchannels(),source.getsampwidth(),source.getframerate())!=(1,2,16000):
                raise ValueError("Clips must be mono 16-bit 16 kHz WAV")
            pcm=source.readframes(source.getnframes())
        for mode in ["fixed","adaptive"]:
            settings=load_settings()|{"endpoint_mode":mode}
            turn=worker(settings,lambda *_:None) if mode=="adaptive" else None
            segmenter=Segmenter(settings["silence_ms"],settings["max_utterance_seconds"],settings=settings,turn_worker=turn)
            clip=None;end_ms=None;started=time.monotonic()
            try:
                for index in range(0,len(pcm)+48000,960):
                    frame=pcm[index:index+960].ljust(960,b"\0")
                    clip=segmenter.feed(frame)
                    if clip:
                        end_ms=(index//960+1)*30;break
                    # Realtime pacing lets nonblocking model checks finish as in capture.
                    await asyncio.sleep(max(0,(index//960+1)*.03-(time.monotonic()-started)))
                row={"wav":case["wav"],"mode":mode,"end_ms":end_ms,"end_delay_ms":None if end_ms is None else end_ms-case["speech_end_ms"],"cut_off":end_ms is None or end_ms<case["speech_end_ms"],"fallback":turn.reason if turn else None}
                if args.transcription_url and clip:
                    import httpx2
                    async with httpx2.AsyncClient(timeout=30,trust_env=False) as http:
                        texts=[]
                        for audio in [pcm,clip]:
                            response=await http.post(args.transcription_url+"/inference",files={"file":("clip.wav",wav_bytes(audio),"audio/wav")},data={"response_format":"json","language":"en"})
                            response.raise_for_status();texts.append(response.json().get("text",""))
                        row["full_text"],row["cut_text"]=texts;row["text_changed"]=texts[0].strip()!=texts[1].strip()
                results.append(row)
            finally:
                if turn:turn.close()
    print(json.dumps(results,indent=2))

if __name__=="__main__":
    parser=argparse.ArgumentParser();parser.add_argument("manifest");parser.add_argument("--transcription-url");asyncio.run(main(parser.parse_args()))
