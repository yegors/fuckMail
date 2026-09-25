#!/usr/bin/env python3
"""Exercise installed local models through the actual engine using synthetic mail only.

Outputs include synthetic summaries, identities, timings and quality mismatches.
Store the output under ignored build/verification, never in a real mailbox profile.
"""
import argparse
import json
import pathlib
import subprocess
import tempfile
import time
from verify_ollama import Client


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--repeat', type=int, default=2)
    parser.add_argument('--model', action='append')
    args = parser.parse_args()
    engine = pathlib.Path(args.engine).resolve()
    cases = json.loads((pathlib.Path(__file__).resolve().parents[1] /
                       'crates/fm-assist/fixtures/ollama-mail.json').read_text(encoding='utf-8'))
    with tempfile.TemporaryDirectory(prefix='fm-local-evaluation-') as tmp, open(args.output, 'w', encoding='utf-8') as output:
        profile = pathlib.Path(tmp) / 'profile'
        profile.mkdir(mode=0o700)
        with open(pathlib.Path(tmp) / 'engine.log', 'w') as log:
            process = subprocess.Popen([str(engine), '--profile', str(profile), '--demo-profile'], stdout=log, stderr=log)
            client = None
            try:
                deadline = time.monotonic() + 30
                while not (profile / 'engine.json').exists():
                    assert process.poll() is None and time.monotonic() < deadline, 'Engine startup failed'
                    time.sleep(.05)
                client = Client(json.loads((profile / 'engine.json').read_text()))
                settings = client.call('assist_settings_get')['settings']
                catalog = client.call('assist_discover', {'endpoint': settings['endpoint']})
                models = [m for m in catalog['models'] if m['eligible'] and
                          (m['name'] in args.model if args.model else m['name'].startswith(('gemma', 'qwen', 'muse-glimmer')))]
                assert models, 'No matching installed models'
                for model in models:
                    settings['model'] = {k: model[k] for k in ('name', 'digest')}
                    for iteration in range(args.repeat):
                        for case in cases:
                            start = time.monotonic()
                            job = client.call('assist_start', {'task': 'message', 'sample': True, 'sample_case': case['id'], 'settings': settings})['job_id']
                            deadline = start + 140
                            while True:
                                result = client.call('assist_job', {'job_id': job})
                                if result['status'] not in ('queued', 'running'):
                                    break
                                assert time.monotonic() < deadline, 'Engine exceeded evaluation watchdog'
                                time.sleep(.04)
                            analysis = result.get('items', [{}])[0].get('output', {}) if result.get('items') else {}
                            row = {'model': model['name'], 'digest': model['digest'], 'case': case['id'], 'iteration': iteration,
                                   'seconds': round(time.monotonic()-start, 3), 'result': result,
                                   'kind_matches': analysis.get('sender_kind') == case['expected_kind'],
                                   'labels_match': analysis.get('category_ids') == case['expected_labels']}
                            output.write(json.dumps(row, ensure_ascii=False)+'\n')
                            output.flush()
                            print(json.dumps({k: v for k, v in row.items() if k not in ('result', 'digest')}, ensure_ascii=False), flush=True)
            finally:
                if client:
                    client.close()
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    process.terminate()
                    process.wait(timeout=10)


if __name__ == '__main__':
    main()
