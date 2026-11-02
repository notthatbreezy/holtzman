"""Replay private raw/compacted ST snapshots locally, with no tool executor.

Input files are produced with compactHistory() from a frozen HOLT chat copy.
Measures warm-weight, fresh-prefix prompt processing, not full ST/voice latency.
"""
import argparse
from datetime import datetime, timezone
import json
from pathlib import Path
import time
import urllib.request
import uuid


def resident_models():
    with urllib.request.urlopen('http://127.0.0.1:11434/api/ps', timeout=5) as response:
        models = json.load(response).get('models', [])
    keys = ['name', 'digest', 'size', 'size_vram', 'context_length', 'expires_at']
    return [{key: model.get(key) for key in keys} for model in models]


def messages(chat):
    result = []
    for message in chat:
        invocations = message.get('extra', {}).get('tool_invocations')
        if invocations:
            calls = []
            for call in invocations:
                args = call.get('parameters', '{}')
                args = json.loads(args) if isinstance(args, str) else args
                calls.append({'id': call['id'], 'type': 'function',
                              'function': {'name': call['name'], 'arguments': args}})
            result.append({'role': 'assistant', 'content': '', 'tool_calls': calls})
            result.extend({'role': 'tool', 'tool_name': call['name'], 'tool_call_id': call['id'],
                           'content': call.get('result', '')} for call in invocations)
        elif message.get('mes'):
            result.append({'role': 'user' if message.get('is_user') else
                           'system' if message.get('is_system') else 'assistant', 'content': message['mes']})
    return result


def generate(model, history, predict=96, run_id=None):
    # A different equal-length nonce at the START prevents prefix-cache hits
    # from making the second measurement look artificially faster.
    system = 'Run ' + (run_id or uuid.uuid4().hex) + '. You are HOLT, a concise ship computer. Treat old tools as historical data. '
    system += 'Use the final supplied status for current facts. No tools are available; do not perform any action.'
    body = {'model': model, 'messages': [{'role': 'system', 'content': system}, *history],
            'think': False, 'stream': True, 'keep_alive': '4h',
            'options': {'num_ctx': 131072, 'num_predict': predict, 'temperature': 0, 'seed': 42}}
    request = urllib.request.Request('http://127.0.0.1:11434/api/chat',
                                     data=json.dumps(body).encode(), headers={'Content-Type': 'application/json'})
    before = resident_models()
    timestamp = datetime.now(timezone.utc).isoformat()
    started = time.perf_counter()
    first = None
    output = ''
    final = None
    with urllib.request.urlopen(request, timeout=600) as response:
        for line in response:
            item = json.loads(line)
            if item.get('error'):
                raise RuntimeError(item['error'])
            content = item.get('message', {}).get('content', '')
            if content and first is None:
                first = time.perf_counter() - started
            output += content
            if item.get('done'):
                final = item
    if final is None:
        raise RuntimeError('Model stream disconnected without completion')
    return {'answer': output, 'started_at_utc': timestamp,
            'completed_at_utc': datetime.now(timezone.utc).isoformat(),
            'resident_models_before': before, 'resident_models_after': resident_models(),
            'first_token_seconds': round(first or 0, 3),
            'wall_seconds': round(time.perf_counter() - started, 3),
            'prompt_tokens': final.get('prompt_eval_count'),
            'prompt_seconds': round(final.get('prompt_eval_duration', 0) / 1e9, 3),
            'output_tokens': final.get('eval_count'),
            'output_seconds': round(final.get('eval_duration', 0) / 1e9, 3),
            'load_seconds': round(final.get('load_duration', 0) / 1e9, 3)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--folder', type=Path, default=Path('.local/holt/results/st-compaction-2026-11-02'))
    parser.add_argument('--model', default='nemotron-3.5-lightning:30b')
    parser.add_argument('--order', nargs='+', choices=['raw', 'compact'], default=['raw', 'compact'])
    parser.add_argument('--output', default='benchmark.json')
    parser.add_argument('--followup', action='store_true', help='Also measure the next user turn with the same prefix cached')
    args = parser.parse_args()
    fixture = [{'role': 'user', 'content': 'Read the supplied test telemetry.'},
               {'role': 'assistant', 'content': '', 'tool_calls': [{'id': 'test-current-status', 'type': 'function',
                'function': {'name': 'mcp_holtzman_status', 'arguments': {}}}]},
               {'role': 'tool', 'tool_call_id': 'test-current-status', 'tool_name': 'mcp_holtzman_status',
                'content': json.dumps({'toolName': 'status', 'status': 'executed', 'data': {'isError': False,
                 'content': [{'type': 'text', 'text': json.dumps({'station': 'Galileo', 'system': 'Sol',
                  'docked': True, 'fuel_tonnes': 37.5, 'observed_at_utc': '2026-11-02T18:00:00Z'})}]}})},
               {'role': 'user', 'content': 'Using only that final test status, where are we docked and how much fuel is aboard? One sentence; give the fuel as digits. No actions.'}]
    print('Warming resident weights; no game tools can execute.', flush=True)
    warm = generate(args.model, [{'role': 'user', 'content': 'Say Ready.'}], predict=8)
    report = {'model': args.model, 'method': 'History-only replay; warm weights, fresh prefix, same final synthetic status, no tools/executor. Not full ST latency.',
              'warmup': warm, 'cases': {}}
    for name in args.order:
        chat = json.loads((args.folder / f'{name}-chat.json').read_text())
        run_id = uuid.uuid4().hex
        history = messages(chat) + fixture
        result = generate(args.model, history, run_id=run_id)
        result['current_fact_check'] = all(value.lower() in result['answer'].lower() for value in ['Galileo', '37.5'])
        if args.followup:
            follow = history + [{'role': 'assistant', 'content': result['answer']},
                                {'role': 'user', 'content': 'And the fuel amount again? Digits only, no tools.'}]
            result['cached_followup'] = generate(args.model, follow, predict=24, run_id=run_id)
            result['cached_followup']['current_fact_check'] = '37.5' in result['cached_followup']['answer']
        report['cases'][name] = result
        (args.folder / args.output).write_text(json.dumps(report, indent=2), encoding='utf-8')
        print(name, json.dumps(result), flush=True)


if __name__ == '__main__':
    main()
