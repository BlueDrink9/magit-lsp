#!/usr/bin/env python3
"""Test-only stdio LSP observer for the R-EDIT-REAL native-client spike."""
import argparse
import json
import sys


def send(message):
    """Write one framed server message to the client."""
    encoded = json.dumps(message).encode()
    sys.stdout.buffer.write(b'Content-Length: ' + str(len(encoded)).encode() + b'\r\n\r\n' + encoded)
    sys.stdout.buffer.flush()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--record', help='append observed LSP messages as JSONL')
    args = parser.parse_args()
    out = open(args.record, 'a', encoding='utf-8') if args.record else None
    pending = {}
    sequence = 0
    while True:
        headers = {}
        while True:
            line = sys.stdin.buffer.readline()
            if not line:
                return
            if line in (b'\r\n', b'\n'):
                break
            key, value = line.decode('ascii').split(':', 1)
            headers[key.lower()] = value.strip()
        body = sys.stdin.buffer.read(int(headers['content-length']))
        message = json.loads(body)
        if out:
            out.write(json.dumps(message) + '\n')
            out.flush()
        if 'id' in message and 'method' not in message:
            if message['id'] in pending:
                send({'jsonrpc': '2.0', 'id': pending.pop(message['id']),
                      'result': message.get('result')})
            continue
        response = {'jsonrpc': '2.0', 'id': message.get('id')}
        if message.get('method') == 'initialize':
            response['result'] = {'capabilities': {'textDocumentSync': {'openClose': True, 'change': 1, 'save': {'includeText': True}}}}
        elif message.get('method') in ('spike/show', 'spike/apply'):
            sequence += 1
            request_id = 'spike-' + str(sequence)
            pending[request_id] = message['id']
            send({'jsonrpc': '2.0', 'id': request_id,
                  'method': 'window/showDocument' if message['method'] == 'spike/show' else 'workspace/applyEdit',
                  'params': message['params']})
            continue
        elif 'id' in message:
            response['result'] = None
        else:
            continue
        send(response)


if __name__ == '__main__':
    main()
