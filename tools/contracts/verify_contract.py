# Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
"""Validate Core/Desktop wire shapes offline; not a runtime acceptance test."""
import copy
import json
from pathlib import Path
import sys

from jsonschema import Draft202012Validator, FormatChecker
from referencing import Registry, Resource

ROOT = Path(__file__).resolve().parents[2] / 'contracts/core-api/v1'


def snapshot_assembly(entities, validator):
    """Validate complete decoded entity bytes before returning a candidate state."""
    if not entities or len(entities) > 32 or any(len(e) > 1048576 for e in entities):
        raise ValueError('page limit')
    if sum(map(len, entities)) > 16777216:
        raise ValueError('aggregate limit')
    def unique_object(pairs):
        result = {}
        for key, value in pairs:
            if key in result:
                raise ValueError('duplicate JSON key')
            result[key] = value
        return result
    pages = [json.loads(e.decode('utf-8'), object_pairs_hook=unique_object) for e in entities]
    if any(not validator.is_valid(p) for p in pages):
        raise ValueError('schema')
    first = pages[0]
    keys = ('schemaMajor', 'conversationId', 'streamEpoch', 'aggregateRevision',
            'appliedThroughSequence', 'resumeAfter', 'snapshotId', 'pageCount', 'totalBytes')
    if first['pageCount'] != len(pages) or first['totalBytes'] != sum(map(len, entities)):
        raise ValueError('page count or byte total')
    if first['appliedThroughSequence'] != first['resumeAfter']:
        raise ValueError('watermark')
    singleton = first['state']['conversation'], first['state']['lease']
    conversation, lease = singleton
    if (conversation['conversationId'] != first['conversationId'] or conversation['streamEpoch'] != first['streamEpoch']
            or conversation['revision'] != first['aggregateRevision'] or (lease is not None and lease['conversationId'] != first['conversationId'])):
        raise ValueError('singleton reference')
    merged = {k: [] for k in ('tasks', 'pendingApprovals', 'messages')}
    tokens = set()
    for index, page in enumerate(pages):
        if page['pageIndex'] != index or any(page[k] != first[k] for k in keys):
            raise ValueError('page identity')
        if (page['state']['conversation'], page['state']['lease']) != singleton:
            raise ValueError('singleton mismatch')
        token = page['nextPageToken']
        if index == len(pages) - 1:
            if token is not None:
                raise ValueError('last token')
        elif not token or token in tokens:
            raise ValueError('continuation token')
        else:
            tokens.add(token)
        for name in merged:
            merged[name].extend(page['state'][name])
    if any(len(merged[k]) > limit for k, limit in (('tasks', 32), ('pendingApprovals', 128), ('messages', 128))):
        raise ValueError('assembled object limit')
    def unique(items, key):
        ids = [x[key] for x in items]
        if len(set(ids)) != len(ids):
            raise ValueError('duplicate identity')
        return set(ids)
    task_ids = unique(merged['tasks'], 'taskRunId')
    unique(merged['pendingApprovals'], 'approvalId')
    unique(merged['messages'], 'messageId')
    if conversation['activeTaskRunId'] is not None and conversation['activeTaskRunId'] not in task_ids:
        raise ValueError('active task missing')
    all_turns, all_effects, by_task = [], [], {}
    for task in merged['tasks']:
        if task['conversationId'] != first['conversationId']:
            raise ValueError('task conversation')
        turns = unique(task['turns'], 'executionTurnId')
        by_task[task['taskRunId']] = turns
        all_turns.extend(task['turns'])
        all_effects.extend(task['effects'])
        for effect in task['effects']:
            if effect['taskRunId'] != task['taskRunId'] or effect['originExecutionTurnId'] not in turns:
                raise ValueError('effect reference')
    unique(all_turns, 'executionTurnId')
    unique(all_effects, 'effectId')
    for approval in merged['pendingApprovals']:
        if (approval['conversationId'] != first['conversationId'] or approval['taskRunId'] not in by_task
                or approval['executionTurnId'] not in by_task[approval['taskRunId']]):
            raise ValueError('approval reference')
    return merged


def encode_pages(pages):
    """Build synthetic actual entities with a self-consistent UTF-8 total."""
    for _ in range(10):
        entities = [json.dumps(p, ensure_ascii=False, separators=(',', ':')).encode('utf-8') for p in pages]
        total = sum(map(len, entities))
        if all(p['totalBytes'] == total for p in pages):
            return entities
        for p in pages:
            p['totalBytes'] = total
    raise ValueError('entity byte total did not converge')


def critical_recovery(pages, validator):
    if not pages:
        raise ValueError('no recovery pages')
    tasks_all, approvals_all, tokens = [], [], set()
    for index, value in enumerate(pages):
        entity = json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode('utf-8')
        if len(entity) > 1048576 or not validator.is_valid(value):
            raise ValueError('recovery schema or size')
        if (value['pageIndex'] != str(index) or value['recoveryId'] != pages[0]['recoveryId']
                or value['expiresAt'] != pages[0]['expiresAt']):
            raise ValueError('recovery page identity')
        token = value['nextPageToken']
        if index == len(pages) - 1:
            if token is not None: raise ValueError('incomplete recovery')
        elif not token or token in tokens:
            raise ValueError('recovery token')
        else:
            tokens.add(token)
        tasks_all.extend(value['tasks'])
        approvals_all.extend(value['pendingApprovals'])
    tasks = [x['taskRunId'] for x in tasks_all]
    approvals = [x['approvalId'] for x in approvals_all]
    if len(set(tasks)) != len(tasks) or len(set(approvals)) != len(approvals):
        raise ValueError('duplicate recovery identity')
    task_conversations = {x['taskRunId']: x['conversationId'] for x in tasks_all}
    if any(x['taskRunId'] not in tasks or task_conversations[x['taskRunId']] != x['conversationId'] for x in approvals_all):
        raise ValueError('recovery approval task')


def high_water(stored, candidate):
    if not candidate['authenticated']:
        return 'REJECT'
    if (candidate['instanceId'], candidate['storeEpoch']) != (stored['instanceId'], stored['storeEpoch']):
        return 'RECONCILE'
    current, incoming = int(stored['generation']), int(candidate['generation'])
    if incoming < current or (incoming == current and candidate['descriptorDigest'] != stored['descriptorDigest']):
        return 'REJECT'
    return 'ACCEPT'


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

    semantic_dir = ROOT / 'fixtures/semantic'
    bundle = read(semantic_dir / 'assembly.json')
    snapshot_validator = Draft202012Validator({'$ref': model + '#/$defs/Snapshot'}, registry=registry, format_checker=checker)
    other_id = '123e4567-e89b-42d3-a456-426614174099'
    for case in bundle['cases']:
        pages = copy.deepcopy(bundle['pages'])
        mutation = case['mutation']
        if mutation == 'missing': pages.pop()
        elif mutation == 'reverse': pages.reverse()
        elif mutation == 'snapshot': pages[1]['snapshotId'] = other_id
        elif mutation == 'epoch': pages[1]['streamEpoch'] = other_id
        elif mutation == 'singleton': pages[1]['state']['conversation']['title'] = 'changed'
        elif mutation == 'duplicate-task': pages[1]['state']['tasks'].append(copy.deepcopy(pages[0]['state']['tasks'][0]))
        elif mutation == 'duplicate-approval': pages[1]['state']['pendingApprovals'].append(copy.deepcopy(pages[1]['state']['pendingApprovals'][0]))
        elif mutation == 'duplicate-message': pages[1]['state']['messages'].append(copy.deepcopy(pages[1]['state']['messages'][0]))
        elif mutation == 'aggregate-limit':
            for index in range(32):
                task = copy.deepcopy(pages[0]['state']['tasks'][0])
                task['taskRunId'] = f'123e4567-e89b-42d3-a456-{index:012d}'
                task['turns'] = []; task['effects'] = []
                pages[1]['state']['tasks'].append(task)
        elif mutation == 'approval-reference': pages[1]['state']['pendingApprovals'][0]['taskRunId'] = other_id
        elif mutation == 'effect-origin': pages[0]['state']['tasks'][0]['effects'][0]['originExecutionTurnId'] = other_id
        elif mutation == 'active-reference':
            for page in pages: page['state']['conversation']['activeTaskRunId'] = other_id
        elif mutation == 'watermark':
            for page in pages: page['resumeAfter'] = '8'
        elif mutation == 'token': pages[0]['nextPageToken'] = None
        elif mutation not in ('none', 'bytes'): raise ValueError('unknown fixture mutation')
        entities = encode_pages(pages)
        if mutation == 'bytes':
            for page in pages: page['totalBytes'] += 1
            entities = [json.dumps(p, ensure_ascii=False, separators=(',', ':')).encode('utf-8') for p in pages]
        try:
            snapshot_assembly(entities, snapshot_validator)
            actual = True
        except (ValueError, UnicodeError):
            actual = False
        checks.append({'case': 'assembly-' + case['name'], 'expectedValid': case['valid'], 'actualValid': actual, 'passed': actual == case['valid']})

    recovery = read(semantic_dir / 'recovery.json')
    recovery_validator = Draft202012Validator({'$ref': model + '#/$defs/CriticalRecovery'}, registry=registry, format_checker=checker)
    empty = copy.deepcopy(recovery); empty['tasks'] = []; empty['pendingApprovals'] = []
    duplicate = copy.deepcopy(recovery); duplicate['tasks'].append(copy.deepcopy(duplicate['tasks'][0]))
    orphan = copy.deepcopy(recovery); orphan['pendingApprovals'][0]['taskRunId'] = other_id
    inline = copy.deepcopy(recovery); inline['pendingApprovals'][0]['prompt'] = 'not a detail reference'
    first = copy.deepcopy(recovery); first['nextPageToken'] = 'page-two'; first['pendingApprovals'] = []
    second = copy.deepcopy(recovery); second['pageIndex'] = '1'; second['tasks'] = []
    swapped = copy.deepcopy(second); swapped['recoveryId'] = other_id
    missing = copy.deepcopy(first)
    wrong_conversation = copy.deepcopy(recovery); wrong_conversation['pendingApprovals'][0]['conversationId'] = other_id
    many_first = copy.deepcopy(first); many_first['tasks'] = []
    many_second = copy.deepcopy(second); many_second['tasks'] = []; many_second['pendingApprovals'] = []
    for index in range(64):
        task = copy.deepcopy(recovery['tasks'][0])
        task['taskRunId'] = f'123e4567-e89b-42d3-a456-{index:012d}'
        (many_first if index < 32 else many_second)['tasks'].append(task)
    for name, sample, expected in [('populated', [recovery], True), ('empty', [empty], True),
                                   ('multi-page', [first, second], True), ('missing-last', [missing], False),
                                   ('mixed-index', [first, swapped], False), ('duplicate', [duplicate], False),
                                   ('orphan', [orphan], False), ('wrong-conversation', [wrong_conversation], False),
                                   ('64-tasks-two-pages', [many_first, many_second], True), ('inline-prompt', [inline], False)]:
        try:
            critical_recovery(sample, recovery_validator); actual = True
        except ValueError:
            actual = False
        checks.append({'case': 'recovery-' + name, 'expectedValid': expected, 'actualValid': actual, 'passed': actual == expected})
    highwater = read(semantic_dir / 'high-water.json')
    for case in highwater['cases']:
        actual = high_water(highwater['stored'], case)
        checks.append({'case': 'highwater-' + case['name'], 'expected': case['expected'], 'actual': actual, 'passed': actual == case['expected']})

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
    print(json.dumps({'schemaChecks': len(checks), 'failed': len(failures), 'checks': checks,
                      'openapi': 'local reference integrity only',
                      'runtime': 'NOT_RUN'}, ensure_ascii=False, indent=2))
    return 1 if failures else 0


if __name__ == '__main__':
    sys.exit(main())
