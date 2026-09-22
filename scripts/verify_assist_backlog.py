#!/usr/bin/env python3
"""Explicit backlog integration against synthetic mail and a local fake model only."""
import argparse
import datetime
import json
import pathlib
import sqlite3
import subprocess
import tempfile
import threading
import time
from verify_ollama import Client, Handler, Server


def wait(client, job):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline:
        state = client.call('assist_job', {'job_id': job})
        if state['status'] not in ('queued', 'scanning', 'running'):
            return state
        time.sleep(.03)
    raise AssertionError('Backlog watchdog expired')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--engine', required=True)
    parser.add_argument('--cli', required=True)
    args = parser.parse_args()
    server = Server(('127.0.0.1', 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    with tempfile.TemporaryDirectory(prefix='fm-backlog-') as root:
        profile = pathlib.Path(root) / 'profile'
        profile.mkdir(mode=0o700)
        with open(pathlib.Path(root) / 'engine.log', 'w') as log:
            process = subprocess.Popen([str(pathlib.Path(args.engine).resolve()), '--profile', str(profile), '--demo-profile'], stdout=log, stderr=log)
            clients = []
            try:
                deadline = time.monotonic() + 20
                while not (profile / 'engine.json').exists():
                    assert process.poll() is None and time.monotonic() < deadline, 'engine startup'
                    time.sleep(.05)
                discovery = json.loads((profile / 'engine.json').read_text())
                a, b = Client(discovery), Client(discovery)
                clients = [a, b]
                a.call('status')
                db = sqlite3.connect(profile / 'store.sqlite3')
                account = 'windscribe'
                rows = db.execute("SELECT id,conversation_id FROM messages WHERE account=? AND sent=0 AND trashed=0 AND length(body)>0 ORDER BY rowid LIMIT 4", [account]).fetchall()
                assert len(rows) == 4
                db.execute("UPDATE messages SET sent_at='2020-01-01T12:00:00Z'")
                now = datetime.datetime.now(datetime.timezone.utc)
                db.executemany('UPDATE messages SET sent_at=? WHERE account=? AND id=?', [(now.isoformat(), account, m) for m, _ in rows])
                db.commit()
                sent_rows = a.call('conversations', {'folder': 'sent', 'limit': 5}, account)['conversations']
                assert sent_rows, 'synthetic Sent fixture exists'
                for row in sent_rows:
                    assert row['recipient'] == db.execute('SELECT recipient FROM messages WHERE account=? AND conversation_id=? AND sent=1 ORDER BY julianday(sent_at) DESC,id DESC LIMIT 1', (account, row['id'])).fetchone()[0], 'Sent recipient follows selected preview source'
                flags_before = db.execute('SELECT account,id,unread,starred FROM messages ORDER BY rowid').fetchall()
                settings = a.call('assist_settings_get')['settings']
                settings['endpoint'] = 'http://127.0.0.1:' + str(server.server_port)
                settings['model'] = {'name': 'fixture:latest', 'digest': 'a' * 64}
                template = json.loads(settings['prompt'])
                template['messages'][0]['content'] = 'Only my visible system instructions.'
                template['messages'][1]['content'] = '{"labels":{{labels}},"messages":{{messages}},"partial":{{partial}}}'
                template['options']['temperature'] = .25
                settings['prompt'] = json.dumps(template)
                server.template = template
                a.call('assist_settings_save', {'expected_revision': 0, 'settings': settings})
                coverage = wait(a, a.call('assist_backlog', account=account)['job_id'])
                assert coverage['ranges']['week'] == 4 and coverage['local_messages'] > 4, coverage
                assert coverage['google_messages'] is None
                assert server.calls == 0, 'coverage never infers'
                # Save one summary; backlog must only fill its classification, retaining exact bytes.
                target, conversation = rows[0]
                first = a.call('assist_start', {'task': 'summary', 'conversation_id': conversation, 'message_id': target}, account)
                assert wait(a, first['job_id'])['status'] == 'completed'
                saved = db.execute("SELECT result_json FROM local_model_results WHERE account=? AND target=? AND task='summary'", (account, target)).fetchone()[0]
                server.mode = 'delay'; server.started.clear(); server.release.clear()
                job = a.call('assist_batch_start', {'range': 'week'}, account)['job_id']
                assert server.started.wait(10), 'batch inference started'
                assert b.call('assist_batch_start', {'range': 'week'}, account, error=True)['code'] == 'assistance_busy'
                assert b.call('assist_job', {'job_id': job}, error=True)['code'] == 'not_found'
                # Interactive work queues behind one message, not behind the whole mailbox.
                interactive = b.call('assist_start', {'task': 'message', 'conversation_id': rows[-1][1], 'message_id': rows[-1][0]}, account)['job_id']
                assert a.call('status')['schema_version'] == 29, 'same GUI session remains responsive'
                server.mode = 'valid'; server.release.set()
                assert wait(b, interactive)['status'] == 'completed'
                final = wait(a, job)
                assert final['status'] == 'completed' and final['failed'] == 0, final
                assert final['completed'] + final['skipped'] == final['total'] == 4, final
                assert db.execute("SELECT result_json FROM local_model_results WHERE account=? AND target=? AND task='summary'", (account, target)).fetchone()[0] == saved
                assert db.execute('SELECT account,id,unread,starred FROM messages ORDER BY rowid').fetchall() == flags_before
                coverage = wait(a, a.call('assist_backlog', account=account)['job_id'])
                assert coverage['ranges']['week'] == 0, coverage
                calls = server.calls
                assert wait(a, a.call('assist_batch_start', {'range': 'week'}, account)['job_id'])['total'] == 0
                assert server.calls == calls, 'finished backlog is not regenerated'
                assert db.execute("SELECT count(*) FROM local_model_results WHERE account!=?", [account]).fetchone()[0] == 0
                # CLI shares counts, waits through scanning, and does no extra work on a complete range.
                for command in [('backlog',), ('batch', '--range', 'week')]:
                    result = subprocess.run([str(pathlib.Path(args.cli).resolve()), '--json', '--profile', str(profile), '--account', account, 'assist', *command], capture_output=True, text=True, timeout=30)
                    assert result.returncode == 0, result.stderr
                    assert json.loads(result.stdout)['result']['status'] == 'completed', result.stdout
                assert server.calls == calls
                # A canceled held generation must not commit; account/model changes fence the next job.
                db.execute('DELETE FROM local_model_results WHERE account=? AND target=?', (account, target)); db.commit()
                server.mode = 'delay'; server.release.clear(); server.started.clear()
                job = a.call('assist_batch_start', {'range': 'week'}, account)['job_id']
                assert server.started.wait(10)
                a.call('assist_cancel', {'job_id': job})
                assert wait(a, job)['status'] == 'canceled'
                server.release.set(); time.sleep(.15)
                assert db.execute('SELECT count(*) FROM local_model_results WHERE account=? AND target=?', (account, target)).fetchone()[0] == 0
                server.release.clear(); server.started.clear()
                job = a.call('assist_batch_start', {'range': 'week'}, account)['job_id']
                assert server.started.wait(10)
                a.call('assist_settings_save', {'expected_revision': 1, 'settings': settings})
                assert wait(a, job)['status'] == 'canceled'
                server.release.set(); time.sleep(.15)
                assert db.execute('SELECT count(*) FROM local_model_results WHERE account=? AND target=?', (account, target)).fetchone()[0] == 0
                server.release.clear(); server.started.clear()
                job = a.call('assist_batch_start', {'range': 'week'}, account)['job_id']
                assert server.started.wait(10)
                a.close(); clients.remove(a)
                server.release.set(); time.sleep(.2)
                assert db.execute('SELECT count(*) FROM local_model_results WHERE account=? AND target=?', (account, target)).fetchone()[0] == 0, 'disconnect cancels uncommitted work'
                assert b.call('status')['schema_version'] == 29, 'independent owner survives'
                # Invalid per-message output is counted, while later eligible messages still run.
                db.execute('DELETE FROM local_model_results WHERE account=? AND target=?', (account, rows[1][0])); db.commit()
                server.mode = 'invalid'
                failed = wait(b, b.call('assist_batch_start', {'range': 'week'}, account)['job_id'])
                assert failed['status'] == 'completed_with_errors' and failed['failed'] == 2 and failed['completed'] == 0, failed
                server.mode = 'valid'
                retry = wait(b, b.call('assist_batch_start', {'range': 'week'}, account)['job_id'])
                assert retry['status'] == 'completed' and retry['completed'] == 2, retry
                assert db.execute('SELECT account,id,unread,starred FROM messages ORDER BY rowid').fetchall() == flags_before
                db.close()
                print(json.dumps({'ok': True, 'checks': ['Sent recipients', 'real coverage', 'missing-only', 'account isolation', 'summary byte preservation', 'review flags unchanged', 'interactive inference', 'session responsiveness', 'owner privacy', 'duplicate exclusion', 'CLI parity', 'empty rerun', 'Stop', 'settings fence', 'disconnect', 'partial failures', 'explicit retry'], 'batch': final, 'model_calls': server.calls}, indent=2))
            finally:
                server.release.set()
                for client in clients: client.close()
                try: process.wait(timeout=5)
                except subprocess.TimeoutExpired: process.terminate(); process.wait(timeout=5)
    server.shutdown()

if __name__ == '__main__':
    main()
