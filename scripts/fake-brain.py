#!/usr/bin/env python3
"""A fake OpenAI-compatible brain for the Test Bench.

    python3 scripts/fake-brain.py --port 18080 --corpus native/test-bench/corpus.txt

Ghost Text (/v1/completions) continues the text from the corpus: it finds the longest end of the prompt that
appears in the corpus and streams the next few words, so a typist that types corpus sentences gets suggestions
it can type through, accept or ignore. Chat (/v1/chat/completions) answers with a fixed short text. Listens on
127.0.0.1 only. GET /stats returns request counts, including requests BOB cancelled by hanging up.
Standard library only.
"""

import argparse
import hashlib
import json
import random
import re
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

STATS = {"completions": 0, "chat": 0, "cancelled": 0, "empty": 0}
LOCK = threading.Lock()


def count(key):
    with LOCK:
        STATS[key] += 1


class Brain:
    def __init__(self, corpus, first_ms, token_ms, words):
        self.text = " ".join(line.strip() for line in corpus.splitlines() if line.strip())
        self.first_ms = first_ms
        self.token_ms = token_ms
        self.words = words

    def continuation(self, prompt):
        """The corpus text after the longest end of `prompt` found in it (6 to 80 characters), a few words long."""
        if "Missing: [" in prompt[-12:]:
            return ""  # gap prompt: no suggestion inside a sentence
        tail = prompt[-80:]
        for start in range(len(tail) - 6 + 1):
            needle = tail[start:]
            at = self.text.find(needle)
            if at >= 0:
                rest = self.text[at + len(needle):]
                break
        else:
            return ""
        # Seeded by the prompt, so the same text always gets the same suggestion.
        rng = random.Random(hashlib.sha256(prompt.encode()).digest())
        n = rng.randint(*self.words)
        pieces = re.findall(r"\s*\S+", rest)[:n]
        out = "".join(pieces)
        end = re.search(r"[.?!]", out)
        return out[: end.end()] if end else out

    def tokens(self, text):
        """Split like a tokenizer would: a leading space stays with its word, long words split in two."""
        out = []
        for piece in re.findall(r"\s*\S+|\s+", text):
            if len(piece) > 6:
                out += [piece[:4], piece[4:]]
            else:
                out.append(piece)
        return out


class Handler(BaseHTTPRequestHandler):
    brain: Brain = None
    protocol_version = "HTTP/1.1"

    def log_message(self, *_):  # quiet: prompts are never printed
        pass

    def body(self):
        n = int(self.headers.get("Content-Length") or 0)
        try:
            return json.loads(self.rfile.read(n) or b"{}")
        except ValueError:
            return {}

    def do_GET(self):
        if self.path == "/stats":
            with LOCK:
                self.json(200, dict(STATS))
        elif self.path == "/v1/models":
            self.json(200, {"object": "list", "data": [{"id": "fake", "object": "model"}]})
        else:
            self.json(404, {"error": "not found"})

    def do_POST(self):
        req = self.body()
        if self.path == "/v1/completions":
            count("completions")
            text = self.brain.continuation(str(req.get("prompt", "")))
            if not text:
                count("empty")
            self.stream([{"choices": [{"text": t, "index": 0}]} for t in self.brain.tokens(text)])
        elif self.path == "/v1/chat/completions":
            count("chat")
            self.stream([{"choices": [{"delta": {"content": t}}]} for t in ["The ", "fake ", "brain ", "answered."]])
        else:
            self.json(404, {"error": "not found"})

    def json(self, status, value):
        data = json.dumps(value).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def stream(self, events):
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Connection", "close")
        self.end_headers()
        try:
            time.sleep(self.brain.first_ms / 1000)
            for e in events:
                self.wfile.write(f"data: {json.dumps(e)}\n\n".encode())
                self.wfile.flush()
                time.sleep(self.brain.token_ms / 1000)
            self.wfile.write(b"data: [DONE]\n\n")
            self.wfile.flush()
        except (BrokenPipeError, ConnectionResetError):
            count("cancelled")
        self.close_connection = True


def main():
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("--port", type=int, default=18080)
    p.add_argument("--corpus", required=True)
    p.add_argument("--first-ms", type=int, default=120, help="time to the first token")
    p.add_argument("--token-ms", type=int, default=15, help="time between tokens")
    p.add_argument("--min-words", type=int, default=2)
    p.add_argument("--max-words", type=int, default=6)
    a = p.parse_args()
    with open(a.corpus, encoding="utf-8") as f:
        Handler.brain = Brain(f.read(), a.first_ms, a.token_ms, (a.min_words, a.max_words))
    server = ThreadingHTTPServer(("127.0.0.1", a.port), Handler)
    print(f"fake brain on 127.0.0.1:{a.port}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass


if __name__ == "__main__":
    main()
