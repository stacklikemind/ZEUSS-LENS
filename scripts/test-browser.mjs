#!/usr/bin/env node
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { WasmClient, generateWithRetries } from '../web/engine.mjs';

const optics = Object.freeze({ op: 'generate', left: Object.freeze({ sphere: 0, cylinder: 0, axis: 0, rx_offset_id: 0 }), right: null, axis_policy: 'native', lens_type: 3 });
let calls = [];
let randomCalls = 0;
const identity = () => (++randomCalls).toString(16).padStart(20, '0');
const topology = { ok: false, error: { code: 'TOPOLOGY', message: 'Synthetic retry test' } };
const exhausted = generateWithRetries({ process(request) { calls.push(request); return topology; } }, optics, null, identity);
assert.equal(exhausted.attempts, 64);
assert.equal(calls.length, 64);
assert.equal(new Set(calls.map(call => call.secret_hex)).size, 64);
for (const request of calls) assert.deepEqual({ ...request, secret_hex: undefined }, { ...optics, secret_hex: undefined });
calls = [];
const fixed = generateWithRetries({ process(request) { calls.push(request); return topology; } }, optics, '00010203040506070809', () => { throw new Error('Fixed identity must not use random'); });
assert.equal(fixed.attempts, 1);
assert.equal(calls.length, 1);
assert.equal(calls[0].secret_hex, '00010203040506070809');
let invalidCalls = 0;
const invalid = generateWithRetries({ process() { invalidCalls += 1; return { ok: false, error: { code: 'INPUT', message: 'Synthetic input error' } }; } }, optics, null, identity);
assert.equal(invalidCalls, 1);
assert.equal(invalid.attempts, 1);
let eventualCalls = 0;
assert.equal(generateWithRetries({ process() { return ++eventualCalls < 3 ? topology : { ok: true, result: {} }; } }, optics, null, identity).attempts, 3);

// Force both allocation and processing to grow memory, catching stale typed-array views.
const memory = new WebAssembly.Memory({ initial: 1 });
const expectedInput = JSON.stringify({ op: 'config' });
let freed = false;
const responseBytes = new TextEncoder().encode('{"ok":true,"result":{"test":true}}');
const fake = new WasmClient({ exports: {
  memory, vc_abi_version: () => 1,
  vc_alloc(length) { assert.equal(length, expectedInput.length); memory.grow(1); return 16; },
  vc_process(pointer, length) { assert.equal(new TextDecoder().decode(new Uint8Array(memory.buffer, pointer, length)), expectedInput); memory.grow(1); new Uint8Array(memory.buffer, 65536, responseBytes.length).set(responseBytes); return 65536; },
  vc_response_len: () => responseBytes.length,
  vc_free(pointer, length) { assert.equal(pointer, 16); assert.equal(length, expectedInput.length); freed = true; },
} });
assert.deepEqual(fake.process({ op: 'config' }), { ok: true, result: { test: true } });
assert.ok(freed);
assert.throws(() => fake.process({ oversized: 'x'.repeat(65536) }), /64 KiB/);
console.log('Browser adapter: bounded topology retry, fixed identity, no optical mutations, ABI memory growth and input bound passed.');

let bytes;
try { bytes = await readFile(new URL('../web/vision_code.wasm', import.meta.url)); }
catch (error) { if (error.code !== 'ENOENT') throw error; }
if (bytes) {
  const { instance } = await WebAssembly.instantiate(bytes, {});
  const client = new WasmClient(instance);
  assert.equal(client.process({ op: 'config' }).ok, true);
  const unsupported = {op:'generate', left:{sphere:-11,cylinder:-3.75,axis:170,rx_offset_id:0}, right:null, axis_policy:'native'};
  const preview = client.process({op:'review', left:unsupported.left, right:null, axis_policy:'native'});
  assert.equal(preview.ok,true);
  assert.equal(preview.result.needs_approximation,true);
  assert.equal(preview.result.eyes.left.encoded.cylinder,-1.75);
  assert.equal(preview.result.eyes.left.calibration.sphere,-10);
  assert.equal('payload_hex' in preview.result,false);
  assert.equal(generateWithRetries(client,unsupported,null,identity).response.ok,false);
  const approximate = generateWithRetries(client,{...unsupported,diopter_policy:'reduce_cylinder'},null,identity);
  assert.equal(approximate.response.ok,true);
  assert.equal(approximate.request.left.cylinder,-3.75);
  assert.equal(approximate.response.result.entered.left.cylinder,-3.75);
  assert.equal(approximate.response.result.encoded.left.cylinder,-1.75);
  assert.equal(approximate.response.result.encoded.left.axis,172);
  assert.equal(approximate.response.result.profile_adjustments[0].entered,-3.75);
  assert.equal(client.process({op:'review',left:unsupported.left,axis_policy:'exact'}).result.can_generate,false);
  assert.equal(client.process({op:'review',left:{...unsupported.left,sphere:-20}}).result.can_generate,false);
  assert.equal(client.process({op:'review'}).result.can_generate,false);
  console.log('Built WASM: explicit approximation, entered values, calibration preview, exact-axis rejection and unavailable profiles passed.');
  const fixtures = JSON.parse(await readFile(new URL('../fixtures/symbols.json', import.meta.url), 'utf8'));
  for (const fixture of fixtures.vectors) {
    const response = client.process({ op: 'encode_payload', payload_hex: fixture.payload_hex });
    assert.equal(response.ok, fixture.ok, fixture.payload_hex);
    if (response.ok) {
      assert.equal(response.result.payload_hex, fixture.payload_hex);
      assert.deepEqual(response.result.gaps, fixture.gaps);
      assert.deepEqual(response.result.colors, fixture.colors);
      const decoded = client.process({ op: 'decode_symbol', gaps: response.result.gaps, colors: response.result.colors });
      assert.equal(decoded.ok, true);
      assert.equal(decoded.result.payload_hex, fixture.payload_hex);
    }
  }
  console.log(`Built WASM: all ${fixtures.vectors.length} symbol fixtures and successful-symbol decode round trips passed.`);
} else {
  console.log('Built WASM not found; run scripts/build.sh and rerun this test for WASM interoperability checks.');
}
