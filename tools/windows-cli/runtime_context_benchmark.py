# Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
"""Test-only native benchmark. Synthetic responses are not upstream performance."""
import argparse
import ctypes
from ctypes import wintypes
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import queue
import statistics
import subprocess
import threading
import time

REQUIRED = 'BENCH_REQUIRED_PERMISSION_AND_USER_CORRECTION'
DETAILS = {'bench-active': 'BENCH_ACTIVE_DETAIL ' * 3000,
           'bench-dependency': 'BENCH_DEPENDENCY_DETAIL ' * 2500,
           'bench-completed': 'BENCH_COMPLETED_DETAIL ' * 18000}


def encode(value):
    return json.dumps(value, separators=(',', ':')).encode()


def save(path, value):
    temporary = path.with_suffix('.tmp')
    temporary.write_text(json.dumps(value, indent=2) + '\n', encoding='utf-8')
    temporary.replace(path)


def output_of(body, call_id):
    item = next(x for x in body['input'] if x.get('call_id') == call_id
                and x.get('type') == 'function_call_output')
    return json.loads(item['output'])


class Provider(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, indexed):
        self.indexed, self.rows, self.errors = indexed, [], []
        super().__init__(('127.0.0.1', 0), Handler)
        self.thread = threading.Thread(target=self.serve_forever, daemon=True)
        self.thread.start()

    def response(self, body, wire):
        stage = len(self.rows)
        if stage == 0:
            items = [dict(type='message', role='assistant', id=k,
                          content=[dict(type='output_text', text=v)]) for k, v in DETAILS.items()]
        elif stage == 1:
            items = [self.call('discover', {'action': 'list', 'include_unrelated': True, 'limit': 100})]
        elif stage == 2:
            discovered = output_of(body, 'discover')
            assert discovered['revision'] == 0
            assert set(DETAILS).issubset({s['source']['id'] for s in discovered['sources']})
            operations = []
            for task, source in zip(('a', 'b', 'c'), DETAILS):
                operations.extend([dict(op='create_task', task_id=task, label='Benchmark ' + task),
                                   dict(op='bind_sources', task_id=task, source_ids=[source])])
            operations.extend([dict(op='set_task_state', task_id='b', state='completed'),
                               dict(op='add_dependency', task_id='a', dependency_id='b')])
            if self.indexed:
                operations.extend([dict(op='set_task_state', task_id='c', state='completed'),
                                   dict(op='set_focus', task_id='a')])
            items = [self.call('apply', dict(action='apply', expected_revision=0, operations=operations))]
        elif stage == 3:
            items = [self.call('verify', {'action': 'list', 'include_unrelated': True, 'limit': 100})]
        else:
            if stage == 4:
                assert output_of(body, 'verify')['revision'] == 1
            actual = encode(body['input'])
            assert REQUIRED.encode() in encode(body)
            for source in ('bench-active', 'bench-dependency'):
                assert DETAILS[source].encode() in actual
            assert (DETAILS['bench-completed'].encode() in actual) == (not self.indexed)
            items = [dict(type='message', role='assistant', id='bench-reply-' + str(stage),
                          content=[dict(type='output_text', text='Benchmark turn complete.')])]
        self.rows.append(dict(sequence=stage, wireBytes=len(wire), inputBytes=len(encode(body['input'])),
                              sha256=hashlib.sha256(wire).hexdigest(), cacheUsage=None))
        response_id = 'bench-response-' + str(stage)
        return ([dict(type='response.created', response=dict(id=response_id))]
                + [dict(type='response.output_item.done', item=item) for item in items]
                + [dict(type='response.completed', response=dict(id=response_id))])

    @staticmethod
    def call(call_id, arguments):
        return dict(type='function_call', name='context_index', call_id=call_id,
                    arguments=json.dumps(arguments))

    def close(self):
        self.shutdown()
        self.server_close()
        self.thread.join(timeout=5)


class Handler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_POST(self):
        self.connection.settimeout(10)
        try:
            assert self.path == '/v1/responses'
            assert not any(self.headers.get(key) for key in ('Authorization', 'Proxy-Authorization', 'Cookie', 'Content-Encoding'))
            length = int(self.headers['Content-Length'])
            assert 0 < length <= 16 * 1024 * 1024
            wire = self.rfile.read(length)
            assert len(wire) == length
            events = self.server.response(json.loads(wire), wire)
            data = b''.join(b'event: ' + e['type'].encode() + b'\ndata: ' + encode(e) + b'\n\n' for e in events)
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.send_header('Content-Length', str(len(data)))
            self.send_header('Connection', 'close')
            self.end_headers()
            self.wfile.write(data)
        except Exception as exc:
            self.server.errors.append(type(exc).__name__ + ': ' + str(exc))
            self.send_error(500, 'Benchmark assertion failed')


class MemoryCounters(ctypes.Structure):
    _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD)] + [
        (key, ctypes.c_size_t) for key in ('PeakWorkingSetSize', 'WorkingSetSize', 'QuotaPeakPagedPoolUsage',
        'QuotaPagedPoolUsage', 'QuotaPeakNonPagedPoolUsage', 'QuotaNonPagedPoolUsage',
        'PagefileUsage', 'PeakPagefileUsage', 'PrivateUsage')]


class IoCounters(ctypes.Structure):
    _fields_ = [(key, ctypes.c_ulonglong) for key in ('ReadOperationCount', 'WriteOperationCount',
                'OtherOperationCount', 'ReadTransferCount', 'WriteTransferCount', 'OtherTransferCount')]


def process_sample(process):
    memory, io = MemoryCounters(), IoCounters()
    memory.cb = ctypes.sizeof(memory)
    handle = wintypes.HANDLE(int(process._handle))
    if not ctypes.windll.psapi.GetProcessMemoryInfo(handle, ctypes.byref(memory), memory.cb):
        raise ctypes.WinError()
    if not ctypes.windll.kernel32.GetProcessIoCounters(handle, ctypes.byref(io)):
        raise ctypes.WinError()
    return dict(privateBytes=memory.PrivateUsage, workingSetBytes=memory.WorkingSetSize,
                readBytes=io.ReadTransferCount, writeBytes=io.WriteTransferCount)


class Client:
    def __init__(self, exe, home, cwd, stderr):
        env = {k: v for k, v in os.environ.items() if not k.upper().startswith(
            ('CODEX', 'ARCVERA', 'OPENAI', 'ANTHROPIC', 'AZURE')) and k.upper() != 'CHATGPT_API_KEY'}
        for key in ('HOME', 'USERPROFILE', 'ARCVERA_HOME', 'CODEX_HOME', 'TEMP', 'TMP'):
            env[key] = str(home)
        for key, suffix in (('APPDATA', 'AppData/Roaming'), ('LOCALAPPDATA', 'AppData/Local')):
            directory = home / suffix
            directory.mkdir(parents=True, exist_ok=True)
            env[key] = str(directory)
        env.update(CODEX_TEST_DISABLE_MANAGED_CONFIG='1', RUST_LOG='warn')
        self.messages, self.request_id, self.compactions = queue.Queue(), 0, 0
        self.process = subprocess.Popen([str(exe), '--no-remote-control', '--disable-plugin-startup-tasks-for-tests'],
            cwd=cwd, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=stderr,
            creationflags=subprocess.CREATE_NO_WINDOW)
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()
        try:
            self.request('initialize', dict(clientInfo=dict(name='runtime-benchmark', version='1.1.0'),
                capabilities=dict(experimentalApi=True, extensions={'arcvera/core': {'version': '1.1.0'}})))
            self.send(dict(method='initialized'))
        except BaseException as exc:
            try:
                self.close()
            except BaseException as cleanup:
                exc.add_note('Cleanup also failed: ' + repr(cleanup))
            raise

    def send(self, item):
        self.process.stdin.write(encode(item) + b'\n')
        self.process.stdin.flush()

    def read(self):
        try:
            for line in self.process.stdout:
                self.messages.put(json.loads(line))
        except Exception as exc:
            self.messages.put(exc)
        finally:
            self.messages.put(EOFError('app-server stdout ended'))

    def receive(self, deadline):
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise TimeoutError('app-server deadline')
        item = self.messages.get(timeout=remaining)
        if isinstance(item, Exception):
            raise item
        if 'id' in item and 'method' in item:
            raise AssertionError('Unexpected server request: ' + item['method'])
        if item.get('method') == 'error':
            raise AssertionError(item)
        if item.get('method') == 'item/completed' and item['params']['item']['type'] == 'contextCompaction':
            self.compactions += 1
        return item

    def request(self, method, params):
        self.request_id += 1
        expected = self.request_id
        self.send(dict(id=expected, method=method, params=params))
        deadline = time.monotonic() + 60
        while True:
            item = self.receive(deadline)
            if item.get('id') == expected:
                assert 'error' not in item, item
                return item['result']

    def turn(self, thread, prompt):
        result = self.request('turn/start', dict(threadId=thread, input=[dict(type='text', text=prompt)], environments=[]))
        expected, deadline = result['turn']['id'], time.monotonic() + 60
        while True:
            item = self.receive(deadline)
            if item.get('method') == 'turn/completed' and item['params']['turn']['id'] == expected:
                assert item['params']['turn']['status'] == 'completed', item
                return

    def close(self):
        self.process.stdin.close()
        try:
            code = self.process.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.process.terminate()
            self.process.wait(timeout=10)
            raise AssertionError('app-server did not exit on stdin EOF')
        self.reader.join(timeout=5)
        assert code == 0, ('app-server exit', code)


def measured_turn(client, provider, thread, prompt):
    before = process_sample(client.process)
    count = len(provider.rows)
    compactions = client.compactions
    started = time.perf_counter_ns()
    client.turn(thread, prompt)
    turn_ns = time.perf_counter_ns() - started
    read_started = time.perf_counter_ns()
    context = client.request('thread/context/read', dict(threadId=thread, cursor=0, limit=100))
    read_ns = time.perf_counter_ns() - read_started
    after = process_sample(client.process)
    assert not provider.errors, provider.errors
    assert len(provider.rows) == count + 1, 'Measured turn must make exactly one real provider request'
    return dict(turnNs=turn_ns, contextReadNs=read_ns, context=context,
                process=after, readBytes=after['readBytes'] - before['readBytes'],
                writeBytes=after['writeBytes'] - before['writeBytes'],
                compactions=client.compactions - compactions, request=provider.rows[-1])


def trial(exe, folder, profile, indexed, row):
    folder.mkdir()
    profile.mkdir()
    home, cwd = profile / 'home', profile / 'cwd'
    home.mkdir()
    cwd.mkdir()
    provider = Provider(indexed)
    client = None
    try:
        config = '\n'.join([
            'model = "mock-model"', 'model_provider = "benchmark"',
            'approval_policy = "never"', 'sandbox_mode = "read-only"',
            'model_context_window = 1000000', 'model_auto_compact_token_limit = 900000',
            '[model_providers.benchmark]', 'name = "Local benchmark"',
            'base_url = "http://127.0.0.1:' + str(provider.server_port) + '/v1"',
            'wire_api = "responses"', 'request_max_retries = 0',
            'stream_max_retries = 0', 'stream_idle_timeout_ms = 30000',
        ]) + '\n'
        (home / 'config.toml').write_text(config, encoding='utf-8')
        with (folder / 'app-server-stderr.log').open('wb') as stderr:
            client = Client(exe, home, cwd, stderr)
            result = client.request('thread/start', dict(cwd=str(cwd), baseInstructions=REQUIRED,
                environments=[], experimentalRawEvents=False))
            thread = result['thread']['id']
            client.turn(thread, 'Produce the three benchmark source details.')
            client.turn(thread, 'Discover sources, apply selection, verify the revision, and continue.')
            assert len(provider.rows) == 5 and not provider.errors, provider.errors
            for index in range(3):
                measured_turn(client, provider, thread, 'Benchmark warmup ' + str(index))
            for index in range(30):
                row['warm'].append(measured_turn(client, provider, thread, 'Benchmark sample ' + str(index)))
                save(folder / 'measurements.json', row)
            row['warmCompactions'] = client.compactions
            client.close()
            client = None
            for index in range(12):
                start = time.perf_counter_ns()
                client = Client(exe, home, cwd, stderr)
                initialized = time.perf_counter_ns()
                resumed = client.request('thread/resume', dict(threadId=thread))
                assert resumed['thread']['id'] == thread
                resumed_at = time.perf_counter_ns()
                sample = measured_turn(client, provider, thread, 'Benchmark resume sample ' + str(index))
                sample.update(initializeNs=initialized-start, resumeNs=resumed_at-initialized)
                row['cold'].append(sample)
                save(folder / 'measurements.json', row)
                client.close()
                client = None
        row['providerRequests'] = len(provider.rows)
        row['status'] = 'passed'
    except BaseException as exc:
        row['status'] = 'failed'
        row['error'] = type(exc).__name__ + ': ' + str(exc)
        raise
    finally:
        try:
            if client is not None:
                client.close()
        finally:
            provider.close()
            row['providerErrors'] = provider.errors
            save(folder / 'measurements.json', row)


def summary(rows):
    result = {}
    for variant in ('unfiltered', 'indexed'):
        result[variant] = {}
        for mode in ('warm', 'cold'):
            samples = [s for r in rows if r['variant'] == variant for s in r[mode]]
            metrics = {}
            for key in ('turnNs', 'contextReadNs', 'readBytes', 'writeBytes'):
                values = sorted(s[key] for s in samples)
                metrics[key] = dict(median=statistics.median(values),
                                    p95=values[(95 * len(values) + 99) // 100 - 1]) if values else None
            for key in ('wireBytes', 'inputBytes'):
                values = [s['request'][key] for s in samples]
                metrics[key] = dict(min=min(values), max=max(values), median=statistics.median(values)) if values else None
            result[variant][mode] = dict(count=len(samples), metrics=metrics,
                compactions=sum(s['compactions'] for s in samples),
                peakObservedPrivateBytes=max((s['process']['privateBytes'] for s in samples), default=None))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--manifest', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert os.name == 'nt', 'Windows process counters required'
    assert os.environ.get('ARCVERA_BENCHMARK_OWNED_JOB') == '1', 'Launch only inside the owned Job guard'
    manifest_bytes = args.manifest.read_bytes()
    manifest = json.loads(manifest_bytes)
    exe = Path(manifest['appServer']).resolve(strict=True)
    assert hashlib.sha256(exe.read_bytes()).hexdigest() == manifest['appServerSha256']
    assert manifest['profile'] == 'development', 'This harness uses a debug-only startup isolation flag'
    profile_root = Path(manifest['profileRoot']).resolve()
    assert profile_root.is_relative_to(Path('D:/codex/build'))
    profile_root.mkdir(exist_ok=False)
    args.output.mkdir(parents=True, exist_ok=False)
    result = dict(status='running', manifestSha256=hashlib.sha256(manifest_bytes).hexdigest(),
                  appServerSha256=manifest['appServerSha256'], sourceHead=manifest['sourceHead'],
                  limitations=['Synthetic provider: upstream cache, billing, TTL and latency NOT_MEASURED',
                    'Cold means a fresh Core process; OS filesystem cache is not flushed',
                    'Process memory sampled at turn boundaries; transient peaks require outer Job counters'], rows=[])
    try:
        for pair in range(3):
            for indexed in ([False, True] if pair % 2 == 0 else [True, False]):
                variant = 'indexed' if indexed else 'unfiltered'
                row = dict(pair=pair, variant=variant, status='running', warm=[], cold=[])
                result['rows'].append(row)
                trial(exe, args.output / (str(pair) + '-' + variant),
                      profile_root / (str(pair) + ('i' if indexed else 'u')), indexed, row)
                save(args.output / 'result.json', result)
        result['status'] = 'passed'
    except BaseException as exc:
        result['status'] = 'failed'
        result['error'] = type(exc).__name__ + ': ' + str(exc)
        raise
    finally:
        result['summary'] = summary(result['rows'])
        save(args.output / 'result.json', result)


if __name__ == '__main__':
    main()
