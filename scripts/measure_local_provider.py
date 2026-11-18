"""Explicitly selected UTF-8 page units; real endpoint measurement, no budget creation."""
import argparse, json, time, urllib.request, urllib.parse
p = argparse.ArgumentParser()
p.add_argument('--endpoint', required=True)
p.add_argument('--model', required=True)
p.add_argument('--pages', nargs='+', required=True)
p.add_argument('--chunk-size', type=int, default=2048)
p.add_argument('--units', type=int, default=25)
p.add_argument('--timeout', type=int, default=300)
a = p.parse_args()
u = urllib.parse.urlsplit(a.endpoint)
if u.scheme != "http" or u.hostname not in ("127.0.0.1", "localhost") or not u.port or u.path != "/v1/chat/completions" or u.query or u.fragment or u.username:
    p.error("endpoint must be an explicit loopback chat completions URL")
if not 256 <= a.chunk_size <= 24000 or not 1 <= a.units <= 30 or not 1 <= a.timeout <= 300:
    p.error("chunk-size 256..24000, units 1..30, timeout 1..300 required")
opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
for i, path in enumerate(a.pages[:a.units]):
    text = open(path, encoding='utf-8').read().encode()[:a.chunk_size].decode('utf-8', errors='ignore')
    body = {'model': a.model, 'messages': [{'role':'user','content':'Return JSON with quote copying the following text verbatim: '+text}], 'response_format':{'type':'json_object'}, 'reasoning_effort':'none', 'thinking':{'type':'disabled'}, 'max_tokens':4096, 'stream':False}
    start = time.monotonic()
    record = {'unit':i, 'page':path, 'bytes':len(text.encode())}
    try:
        req = urllib.request.Request(a.endpoint, json.dumps(body).encode(), {'Content-Type':'application/json'})
        with opener.open(req, timeout=a.timeout) as response: result=json.load(response)
        content=json.loads(result['choices'][0]['message']['content'])
        record.update(success=True, quote_exact=content.get('quote')==text, usage=result.get('usage'))
    except Exception as e:
        record.update(success=False, error=str(e))
    record['elapsed_seconds']=time.monotonic()-start
    print(json.dumps(record, ensure_ascii=False), flush=True)
