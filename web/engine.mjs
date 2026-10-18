/** Raw, JSON-over-memory ABI. No optical algorithms run in this adapter. */
export class WasmClient {
  constructor(instance) {
    this.exports = instance.exports;
    for (const name of ['vc_alloc', 'vc_free', 'vc_process', 'vc_response_len', 'vc_abi_version']) {
      if (typeof this.exports[name] !== 'function') throw new Error(`Missing WASM export: ${name}`);
    }
    if (!(this.exports.memory instanceof WebAssembly.Memory) || this.exports.vc_abi_version() !== 1) {
      throw new Error('The WASM module does not implement ABI version 1. Rebuild this project.');
    }
  }

  static async load(url = new URL('./vision_code.wasm', import.meta.url)) {
    const response = await fetch(url);
    if (!response.ok) throw new Error(`Cannot load the local WASM engine (HTTP ${response.status}). Run scripts/build.sh first.`);
    const { instance } = await WebAssembly.instantiate(await response.arrayBuffer(), {});
    return new WasmClient(instance);
  }

  process(request) {
    const input = new TextEncoder().encode(JSON.stringify(request));
    if (input.length > 65536) throw new Error('Request exceeds the 64 KiB input limit.');
    const { exports: wasm } = this;
    const pointer = wasm.vc_alloc(input.length) >>> 0;
    if (!pointer) throw new Error('The WASM engine could not allocate input memory.');
    try {
      // Allocation can grow memory: never reuse an earlier view.
      new Uint8Array(wasm.memory.buffer, pointer, input.length).set(input);
      const outputPointer = wasm.vc_process(pointer, input.length) >>> 0;
      const outputLength = wasm.vc_response_len() >>> 0;
      if (!outputPointer || !outputLength || outputLength > wasm.memory.buffer.byteLength - outputPointer) {
        throw new Error('The WASM engine returned an invalid response buffer.');
      }
      // Processing can also grow memory. Decode the module-owned output before another call.
      const json = new TextDecoder('utf-8', { fatal: true }).decode(new Uint8Array(wasm.memory.buffer, outputPointer, outputLength));
      const response = JSON.parse(json);
      if (typeof response.ok !== 'boolean') throw new Error('The WASM engine returned an invalid response envelope.');
      return response;
    } finally {
      wasm.vc_free(pointer, input.length);
    }
  }
}

export function freshIdentity() {
  const bytes = crypto.getRandomValues(new Uint8Array(10));
  return Array.from(bytes, value => value.toString(16).padStart(2, '0')).join('');
}

/** Optics remain unchanged. Only TOPOLOGY may retry, at most 64 identities total. */
export function generateWithRetries(client, optics, fixedIdentity = null, createIdentity = freshIdentity) {
  const limit = fixedIdentity === null ? 64 : 1;
  for (let attempt = 1; attempt <= limit; attempt += 1) {
    const request = { ...optics, secret_hex: fixedIdentity ?? createIdentity() };
    const response = client.process(request);
    if (response.ok || response.error?.code !== 'TOPOLOGY' || attempt === limit) {
      return { request, response, attempts: attempt, identity_mode: fixedIdentity === null ? 'fresh' : 'fixed' };
    }
  }
  throw new Error('Unexpected generation state.');
}
