#!/usr/bin/env python3
"""A local stand-in for the gallery API (api.gitbonsai.dev/v1/gallery), for building
and checking the site before the real one is live. In memory only.

  python site/dev/stub-api.py --hero <built-site>/hero [--port 8787] [--site URL]
         [--fail] [--empty] [--ratelimit] [--selftest]

It serves the contract the server session confirmed: the list, starred, star toggles,
the SVGs, and a sign-in that comes straight back with a result. A login starting
with "x-" comes back as a mismatch, one starting with "c-" as cancelled.
"""
import argparse, json, re, socket, sys, threading, urllib.error, urllib.parse, urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

LOGIN = re.compile(r"^[A-Za-z0-9](-?[A-Za-z0-9])*$")
SEED = [  # the site's plates: login, style, languages
    ("StevenBtw", "fukinagashi", ["Rust", "Python", "TypeScript"], 450),
    ("jacobwilliams", "kengai", ["Fortran", "Python", "HTML"], 624),
    ("tj", "shakan", [], 936),
    ("torvalds", "chokkan", ["C"], 780),
    ("steipete", "moyogi", ["TypeScript", "JavaScript"], 1040),
    ("showcase", "moyogi", ["Java", "Python", "TypeScript"], 1000),
    ("sprout", "chokkan", ["Python"], 2),
    ("newcomer", "chokkan", ["Python", "TypeScript"], 17),
]


WAITING = ["seedling", "a-long-login-still-waiting-to-grow-up"]


def make_handler(args):
    entries = [
        {"login": l, "github_id": i + 1, "style": s, "languages": langs,
         "added": f"2026-11-{21 - i:02d}T09:00:00Z", "grown": "2026-11-21T09:00:00Z", "stars": i % 3, "age_weeks": w}
        for i, (l, s, langs, w) in enumerate([] if args.empty else SEED)
    ]
    if not args.empty:  # shown, not grown yet: no style, picture or star
        entries[1:1] = [{"login": l, "github_id": 900 + i, "added": f"2026-11-{21 - i:02d}T12:00:00Z", "example": False,
                         "stars": 0, "grown": None, "style": None, "languages": [], "age_weeks": None}
                        for i, l in enumerate(WAITING)]
    stars = set()  # (client ip, lowercased login) pairs that are starred
    lock = threading.Lock()

    def find(login):
        return next((e for e in entries if e["login"].lower() == login.lower()), None)

    class H(BaseHTTPRequestHandler):
        def log_message(self, *a):
            pass

        def send(self, code, body=None, ctype="application/json", extra=()):
            data = body if isinstance(body, bytes) else json.dumps(body).encode() if body is not None else b""
            self.send_response(code)
            self.send_header("Content-Type", ctype)
            origin = self.headers.get("Origin", "")
            if re.match(r"^http://(127\.0\.0\.1|localhost)(:\d+)?$", origin):
                self.send_header("Access-Control-Allow-Origin", origin)
                self.send_header("Access-Control-Expose-Headers", "Retry-After")
            for k, v in extra:
                self.send_header(k, v)
            self.end_headers()
            self.wfile.write(data)

        def do_OPTIONS(self):
            self.send(204, b"", "text/plain", [("Access-Control-Allow-Methods", "GET, POST")])

        def do_GET(self):
            if args.fail:
                return self.send(500, b"<html>oops</html>", "text/html")
            u = urllib.parse.urlsplit(self.path)
            q = dict(urllib.parse.parse_qsl(u.query, keep_blank_values=True))
            ip = self.client_address[0]
            if u.path == "/v1/gallery":
                return self.send(200, {"entries": entries}, extra=[("Cache-Control", "public, max-age=60")])
            if u.path == "/v1/gallery/starred":
                with lock:
                    mine = [e["login"] for e in entries if (ip, e["login"].lower()) in stars]
                return self.send(200, {"starred": mine}, extra=[("Cache-Control", "no-store")])
            if u.path == "/v1/gallery/auth":
                login, action, legend = q.get("login"), q.get("action"), q.get("legend")
                if login is None and action == "add":
                    login = "StevenBtw"  # no username typed: the stub "signs in" as this account
                if not (len(login or "") <= 39 and LOGIN.match(login or "")):
                    return self.send(400, {"error": "invalid_login"})
                if action not in ("add", "remove") or legend not in ("0", "1"):
                    return self.send(400, {"error": "bad_request"})
                flag = "mismatch" if login.startswith("x-") else "cancelled" if login.startswith("c-") else None
                if action == "add":
                    flag = flag or "added"
                    if flag == "added" and not find(login):
                        entries.insert(0, {"login": login, "github_id": 1000 + len(entries), "style": "moyogi",
                                           "languages": ["Python"], "added": "2026-11-22T09:00:00Z",
                                           "grown": "2026-11-22T09:00:00Z", "stars": 0, "age_weeks": 120})
                    loc = f"{args.site}/?gallery={flag}&login={login}{'&legend=1' if legend == '1' else ''}#yours"
                else:
                    flag = flag or "removed"
                    if flag == "removed":
                        entries[:] = [e for e in entries if e["login"].lower() != login.lower()]
                    loc = f"{args.site}/exhibition.html?gallery={flag}&login={login}"
                return self.send(302, b"", "text/plain", [("Location", loc)])
            t = re.match(r"^/v1/tree/([^/]+)$", u.path)
            if t:  # "See your own tree", so a sign-in that comes back can grow the tree again
                e = find(t.group(1)) or {"login": t.group(1), "style": "moyogi"}
                leg = "-legend" if q.get("legend") == "1" else ""
                own = e["login"].lower() if find(t.group(1)) else "showcase"

                def pic(th):
                    try:
                        return open(f"{args.hero}/{own}{leg}-{th}.svg", encoding="utf-8").read()
                    except OSError:  # a tree without a plate of its own, as for its gallery picture
                        return open(f"{args.hero}/showcase{leg}-{th}.svg", encoding="utf-8").read()
                return self.send(200, {"login": e["login"], "observed_on": "2026-W47", "style": e["style"], "gitbonsai": "stub",
                                       "legend": bool(leg), "light": pic("light"), "dark": pic("dark")})
            m = re.match(r"^/v1/gallery/([^/]+)/(legend-)?(light|dark)\.svg$", u.path)
            if m and find(m.group(1)) and find(m.group(1))["grown"]:
                name = m.group(1).lower()  # the plates' files are named in lower case
                path = f"{args.hero}/{name}{'-legend' if m.group(2) else ''}-{m.group(3)}.svg"
                try:
                    data = open(path, "rb").read()
                except OSError:  # a tree added through the stub has no picture of its own
                    data = open(f"{args.hero}/showcase-{m.group(3)}.svg", "rb").read()
                return self.send(200, data, "image/svg+xml", [("Cache-Control", "public, max-age=31536000, immutable")])
            return self.send(404, {"error": "not_found"})

        def do_POST(self):
            if args.fail:
                return self.send(500, b"<html>oops</html>", "text/html")
            m = re.match(r"^/v1/gallery/([^/]+)/star$", urllib.parse.urlsplit(self.path).path)
            if not m:
                return self.send(404, {"error": "not_found"})
            if args.ratelimit:
                return self.send(429, {"error": "rate_limited"}, extra=[("Retry-After", "5")])
            e = find(m.group(1))
            if not e or not e["grown"]:
                return self.send(404, {"error": "not_found"})
            key = (self.client_address[0], e["login"].lower())
            with lock:
                if key in stars:
                    stars.discard(key)
                    e["stars"] -= 1
                else:
                    stars.add(key)
                    e["stars"] += 1
                return self.send(200, {"stars": e["stars"], "starred": key in stars})

    return H


def serve(args):
    return ThreadingHTTPServer(("127.0.0.1", args.port), make_handler(args))


def selftest(args):
    def run(extra=()):
        a = parser.parse_args(["--hero", args.hero, "--port", "0", *extra])
        srv = serve(a)
        threading.Thread(target=srv.serve_forever, daemon=True).start()
        return srv, f"http://127.0.0.1:{srv.server_address[1]}/v1/gallery"

    class NoRedirect(urllib.request.HTTPRedirectHandler):
        def redirect_request(self, *a, **k):
            return None

    def call(url, method="GET"):
        try:
            r = urllib.request.build_opener(NoRedirect).open(urllib.request.Request(url, method=method))
            return r.status, r.headers, r.read()
        except urllib.error.HTTPError as e:
            return e.code, e.headers, e.read()

    srv, B = run()
    code, _, body = call(B)
    lst = json.loads(body)["entries"]
    grown = [e for e in lst if e["grown"]]
    assert code == 200 and len(grown) == 8 and all(len(e) == 8 for e in grown) and len(lst) == 10, lst
    lst = grown
    before = lst[0]["stars"]
    a = json.loads(call(f"{B}/{lst[0]['login']}/star", "POST")[2])
    assert a["starred"] is True and a["stars"] == before + 1, a
    assert json.loads(call(f"{B}/starred")[2])["starred"] == [lst[0]["login"]]
    b = json.loads(call(f"{B}/{lst[0]['login']}/star", "POST")[2])
    assert b["starred"] is False and b["stars"] == before, b
    assert json.loads(call(f"{B}/starred")[2])["starred"] == []
    code, h, _ = call(f"{B}/auth?login=tj&action=add&legend=0")
    assert code == 302 and "gallery=added&login=tj" in h["Location"], (code, dict(h))
    assert call(f"{B}/auth?login=a--b&action=add&legend=0")[0] == 400
    assert call(f"{B}/tj/dark.svg?v=x")[0] == 200 and call(f"{B}/tj/legend-light.svg")[0] == 200
    assert call(f"{B}/seedling/dark.svg")[0] == 404 and call(f"{B}/seedling/star", "POST")[0] == 404
    assert call(f"{B}/nobody/dark.svg")[0] == 404 and call(f"{B}/tj-dark.svg")[0] == 404
    srv.shutdown()
    srv, B = run(["--fail"])
    assert call(B)[0] == 500
    srv.shutdown()
    print("selftest ok")


parser = argparse.ArgumentParser()
parser.add_argument("--hero", required=True, help="a built site's hero/ folder (the plates' SVGs)")
parser.add_argument("--port", type=int, default=8787)
parser.add_argument("--site", default="http://127.0.0.1:8768", help="where the sign-in sends visitors back to")
for flag in ("--fail", "--empty", "--ratelimit", "--selftest"):
    parser.add_argument(flag, action="store_true")

if __name__ == "__main__":
    args = parser.parse_args()
    if args.selftest:
        selftest(args)
    else:
        print(f"stub gallery API on http://127.0.0.1:{args.port}/v1/gallery", file=sys.stderr)
        serve(args).serve_forever()
