"""Replay recorded WAVs through transcription and grammar without executing game tools.

Run explicitly with a JSON manifest [{wav,text,held,expected_actions}]. Needs an
already running transcription server; never starts services or touches devices.
"""
import argparse
import asyncio
import json
from pathlib import Path
import time
import httpx2
from client.fastpath import parse,gate

async def main(manifest,url):
    results=[]
    async with httpx2.AsyncClient(timeout=30,trust_env=False) as http:
        for case in json.loads(Path(manifest).read_text(encoding="utf-8")):
            started=time.monotonic()
            response=await http.post(url+"/inference",files={"file":("clip.wav",Path(case["wav"]).read_bytes(),"audio/wav")},data={"response_format":"verbose_json","language":"en"})
            response.raise_for_status();data=response.json();match=parse(data.get("text",""))
            accepted=bool(match and gate(match,held=case.get("held",False),metadata=data,spoken=True) is None)
            actions=list(match.actions) if accepted else []
            results.append({"wav":case["wav"],"transcript":data.get("text"),"seconds":round(time.monotonic()-started,3),"actions":actions,"pass":actions==case.get("expected_actions",[])})
    print(json.dumps(results,indent=2));return all(r["pass"] for r in results)

if __name__=="__main__":
    parser=argparse.ArgumentParser();parser.add_argument("manifest");parser.add_argument("--url",default="http://127.0.0.1:8081");args=parser.parse_args()
    raise SystemExit(0 if asyncio.run(main(args.manifest,args.url)) else 1)
