"""PipeWire microphone -> Mistral Voxtral Mini Realtime -> MiSide OSC."""

import argparse
import asyncio
import base64
import contextlib
import json
import os
from pathlib import Path
import socket
import struct
import sys
import time
from urllib.parse import urlencode

from websockets.asyncio.client import connect


def osc_string(value):
    data = value.encode() + b"\0"
    return data + b"\0" * (-len(data) % 4)


def osc_int(name, value):
    return (
        osc_string("/avatar/parameters/MiSide/" + name)
        + osc_string(",i")
        + struct.pack(">i", value)
    )


def send_text(sock, target, text, revision):
    for char in text:
        code = ord(char)
        messages = [
            osc_int(name, (code >> shift) & 255)
            for name, shift in [("CodeLo", 0), ("CodeHi", 8), ("CodeTop", 16)]
        ]
        bundle = b"#bundle\0" + struct.pack(">Q", 1)
        bundle += b"".join(struct.pack(">i", len(msg)) + msg for msg in messages)
        sock.sendto(bundle, target)
        time.sleep(0.06)
        revision = (revision + 1) & 255
        sock.sendto(osc_int("Revision", revision), target)
        time.sleep(0.08)
    return revision


async def stream_audio(ws, process):
    while True:
        pcm = await process.stdout.read(3200)
        if not pcm:
            raise RuntimeError("マイク録音が終了しました。PipeWireと入力デバイスを確認してください。")
        await ws.send(json.dumps({
            "type": "input_audio.append",
            "audio": base64.b64encode(pcm).decode("ascii"),
        }))


async def receive_text(ws, pending):
    async for raw in ws:
        event = json.loads(raw)
        if event["type"] == "error":
            raise RuntimeError(f"文字起こしAPIエラー: {event.get('error')}")
        if event["type"] == "transcription.text.delta":
            text = event.get("text", "")
            if text:
                print(text, end="", flush=True)
                await pending.put(text)
        if event["type"] == "transcription.done":
            return
    raise RuntimeError("文字起こしAPIとの接続が切断されました。")


async def send_pending(pending, sock, target):
    revision = 0
    while True:
        text = await pending.get()
        revision = await asyncio.to_thread(send_text, sock, target, text, revision)


async def run(args, api_key):
    url = "wss://api.mistral.ai/v1/audio/transcriptions/realtime?" + urlencode({"model": args.model})
    async with connect(url, additional_headers={"Authorization": "Bearer " + api_key}, open_timeout=20) as ws:
        async with asyncio.timeout(20):
            while True:
                event = json.loads(await ws.recv())
                if event["type"] == "error":
                    raise RuntimeError(f"文字起こしAPIエラー: {event.get('error')}")
                if event["type"] == "session.created":
                    break
        await ws.send(json.dumps({"type": "session.update", "session": {
            "audio_format": {"encoding": "pcm_s16le", "sample_rate": 16000},
        }}))
        command = ["pw-record", "--raw", "--format", "s16", "--rate", "16000", "--channels", "1"]
        if args.device:
            command += ["--target", args.device]
        process = await asyncio.create_subprocess_exec(*command, "-", stdout=asyncio.subprocess.PIPE)
        tasks = []
        with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as sock:
            try:
                print(f"マイク文字起こし開始: {args.model} → OSC {args.osc_host}:{args.osc_port} (Ctrl+Cで終了)", flush=True)
                pending = asyncio.Queue(maxsize=256)
                tasks = [
                    asyncio.create_task(stream_audio(ws, process)),
                    asyncio.create_task(receive_text(ws, pending)),
                    asyncio.create_task(send_pending(pending, sock, (args.osc_host, args.osc_port))),
                ]
                done, _ = await asyncio.wait(tasks, return_when=asyncio.FIRST_COMPLETED)
                for task in done:
                    task.result()
            finally:
                for task in tasks:
                    task.cancel()
                await asyncio.gather(*tasks, return_exceptions=True)
                if process.returncode is None:
                    with contextlib.suppress(ProcessLookupError):
                        process.terminate()
                await process.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--model", default="voxtral-mini-transcribe-realtime-2602")
    parser.add_argument("--device", help="PipeWire入力ノード名またはserial（既定: デフォルトマイク）")
    parser.add_argument("--osc-host", default="127.0.0.1")
    parser.add_argument("--osc-port", type=int, default=9000)
    args = parser.parse_args()
    api_key = os.environ.get("MISTRAL_API_KEY", "").strip()
    key_file = Path.cwd() / ".mistral-api-key"
    if not api_key and key_file.is_file():
        api_key = key_file.read_text().strip()
    if not api_key:
        parser.error("MISTRAL_API_KEY または .mistral-api-key を設定してください")
    try:
        asyncio.run(run(args, api_key))
    except KeyboardInterrupt:
        print()
    except Exception as error:
        print(f"文字起こしを停止しました: {error}", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    main()
