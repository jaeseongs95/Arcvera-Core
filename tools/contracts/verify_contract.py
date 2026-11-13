"""Validate Core/Desktop wire shapes offline; not a runtime acceptance test."""
import copy
import json
from pathlib import Path
import sys

from jsonschema import Draft202012Validator, FormatChecker
from referencing import Registry, Resource

ROOT = Path(__file__).resolve().parents[2] / 'contracts/core-api/v1'


def read(path):
    return json.loads(path.read_text(encoding='utf-8'))


def main():
    schemas = [read(ROOT / name) for name in ('schemas.json', 'events.schema.json', 'bootstrap-descriptor.schema.json')]
    registry = Registry().with_resources((s['$id'], Resource.from_contents(s)) for s in schemas)
    for schema in schemas:
        Draft202012Validator.check_schema(schema)
    checker = FormatChecker()
    checks = []

    def check(name, schema_ref, instance, expected):
        validator = Draft202012Validator({'$ref': schema_ref}, registry=registry, format_checker=checker)
        errors = list(validator.iter_errors(instance))
        checks.append({'case': name, 'expectedValid': expected, 'actualValid': not errors,
                       'passed': (not errors) == expected})

    model = schemas[0]['$id']
    event = schemas[1]['$id']
    manifest = read(ROOT / 'fixtures/manifest.json')
    declared = {c['file'] for c in manifest['cases']}
    found = {p.name for p in (ROOT / 'fixtures').glob('*.json')} - {'manifest.json'}
    if found != declared:
        raise ValueError('Fixture manifest must list exactly every fixture')
    for case in manifest['cases']:
        ref = case['schema'].replace('../schemas.json', model).replace('../events.schema.json', event).replace('../bootstrap-descriptor.schema.json', schemas[2]['$id'])
        check(case['file'], ref, read(ROOT / 'fixtures' / case['file']), case['valid'])

    submit = read(ROOT / 'fixtures/submit-task.valid.json')
    for value, valid in [('0', True), ('7', True), ('9223372036854775807', True),
                         ('9223372036854775808', False), ('10000000000000000000', False),
                         ('01', False), ('-1', False), ('1.0', False), ('1e2', False), (7, False)]:
        sample = copy.deepcopy(submit)
        sample['expectedRevision'] = value
        check('counter-' + str(value) + '-' + type(value).__name__, model + '#/$defs/SubmitTask', sample, valid)
    for field in ('continuation', 'vendorToken', 'rawReasoning'):
        sample = copy.deepcopy(submit)
        sample[field] = 'must not reach the wire'
        check('forbidden-' + field, model + '#/$defs/SubmitTask', sample, False)
    invalid_uuid = copy.deepcopy(submit)
    invalid_uuid['operation']['operationId'] = 'not-a-uuid'
    check('invalid-uuid', model + '#/$defs/SubmitTask', invalid_uuid, False)

    uid = '123e4567-e89b-42d3-a456-426614174001'
    durable = {'kind': 'durable', 'schemaMajor': 1, 'eventId': uid, 'conversationId': uid,
               'taskRunId': None, 'executionTurnId': None, 'streamEpoch': uid, 'sequence': '1',
               'aggregateRevision': '1', 'occurredAt': '2026-11-13T09:00:00.000Z',
               'payload': {'type': 'detailAvailable', 'detailId': uid, 'detailType': 'COMMAND'}}
    transient = {'kind': 'transient', 'schemaMajor': 1, 'conversationId': uid, 'taskRunId': uid,
                 'executionTurnId': uid, 'exchangeId': uid, 'streamFrame': '0',
                 'payload': {'type': 'textDelta', 'messageId': uid, 'text': 'hello'}}
    control = {'kind': 'control', 'schemaMajor': 1, 'conversationId': uid, 'type': 'SNAPSHOT_REQUIRED',
               'code': 'CURSOR_GAP', 'snapshotUri': '/api/v1/conversations/' + uid + '/snapshot'}
    for name, sample in [('durable', durable), ('transient', transient), ('control', control)]:
        check(name, event, sample, True)
        future = copy.deepcopy(sample)
        future['schemaMajor'] = 2
        check(name + '-unknown-major', event, future, False)
    for field in ('sequence', 'streamEpoch', 'eventId'):
        sample = copy.deepcopy(transient)
        sample[field] = durable[field]
        check('transient-forbids-' + field, event, sample, False)
    bad = copy.deepcopy(durable)
    bad['sequence'] = '9223372036854775808'
    check('event-sequence-overflow', event, bad, False)
    bad = copy.deepcopy(control)
    bad['code'] = 'MADE_UP_SUCCESS'
    check('control-unknown-code', event, bad, False)
    check('task-unknown-state', model + '#/$defs/TaskRunState', 'DISCONNECTED_SUCCESS', False)
    turn = {'executionTurnId': uid, 'kind': 'TOOL', 'state': 'PREPARED', 'ownerEpoch': uid,
            'statusRevision': '0', 'connection': 'CONNECTED', 'outcome': 'RESOLVED',
            'callId': 'original-call', 'detailId': None}
    check('tool-original-call-id', model + '#/$defs/Turn', turn, True)
    turn['callId'] = None
    check('tool-null-call-id', model + '#/$defs/Turn', turn, False)
    receipt = {'operationId': uid, 'createdAt': '2026-11-13T09:00:00.000Z',
               'payloadDigest': 'sha256:' + '0' * 64, 'status': 'ACCEPTED', 'resourceType': 'TASK_RUN',
               'resourceId': uid, 'revision': '1', 'errorCode': None}
    check('accepted-receipt', model + '#/$defs/OperationReceipt', receipt, True)
    for field in ('resourceId', 'revision'):
        bad = copy.deepcopy(receipt)
        bad[field] = None
        check('accepted-null-' + field, model + '#/$defs/OperationReceipt', bad, False)
    message = {'messageId': uid, 'role': 'ASSISTANT', 'state': 'COMPLETE',
               'parts': [{'type': 'text', 'text': 'done'}], 'detailIds': []}
    completed = copy.deepcopy(durable)
    completed['payload'] = {'type': 'messageCompleted', 'message': message}
    check('completed-message', event, completed, True)
    completed['payload']['message']['state'] = 'PARTIAL'
    check('completed-message-rejects-partial', event, completed, False)
    message['parts'] = [{'type': 'text', 'text': 'x'}] * 129
    check('message-parts-bounded', model + '#/$defs/Message', message, False)

    # Offline reference integrity only. A full OpenAPI semantic validator is separate.
    api = read(ROOT / 'openapi.json')
    if api['openapi'] != '3.1.0' or not api['paths']:
        raise ValueError('Expected nonempty OpenAPI 3.1 contract')
    def references(value):
        if isinstance(value, dict):
            if '$ref' in value:
                target, _, pointer = value['$ref'].partition('#')
                doc = api if not target else read(ROOT / target)
                for part in pointer.lstrip('/').split('/') if pointer else []:
                    doc = doc[part.replace('~1', '/').replace('~0', '~')]
            for child in value.values():
                references(child)
        elif isinstance(value, list):
            for child in value:
                references(child)
    references(api)
    event_get = api['paths']['/api/v1/conversations/{conversationId}/events']['get']
    after = next(p['schema'] for p in event_get['parameters'] if p['name'] == 'after')
    after_validator = Draft202012Validator(after, registry=registry, format_checker=checker)
    overflow_invalid = not after_validator.is_valid('9223372036854775808')
    checks.append({'case': 'query-after-int64', 'expectedValid': False,
                   'actualValid': not overflow_invalid, 'passed': overflow_invalid})
    failures = [c for c in checks if not c['passed']]
    failures = [c for c in checks if not c['passed']]
    print(json.dumps({'schemaChecks': len(checks), 'failed': len(failures), 'checks': checks,
                      'openapi': 'local reference integrity only',
                      'runtime': 'NOT_RUN'}, ensure_ascii=False, indent=2))
    return 1 if failures else 0


if __name__ == '__main__':
    sys.exit(main())
