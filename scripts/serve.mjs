#!/usr/bin/env node
import { createServer } from 'node:http';
import { readFile, stat, realpath } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import { resolve, sep, extname } from 'node:path';

const root = await realpath(fileURLToPath(new URL('../web/', import.meta.url)));
const port = Number(process.env.PORT ?? '8766');
if (!Number.isInteger(port) || port < 1 || port > 65535) throw new Error('PORT must be an integer between 1 and 65535.');
const mime = { '.html': 'text/html; charset=utf-8', '.css': 'text/css; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.png': 'image/png', '.json': 'application/json; charset=utf-8' };
const server = createServer(async (request, response) => {
  response.setHeader('X-Content-Type-Options', 'nosniff');
  response.setHeader('Referrer-Policy', 'no-referrer');
  response.setHeader('Cache-Control', 'no-store');
  response.setHeader('Content-Security-Policy', "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' blob: data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'");
  if (!['GET', 'HEAD'].includes(request.method)) {
    response.writeHead(405, { Allow: 'GET, HEAD' });
    response.end('Static files only.');
    return;
  }
  try {
    const pathname = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
    if (pathname.includes('\0') || pathname.includes('\\')) throw new Error('Invalid path');
    const requestedPath = resolve(root, `.${pathname.endsWith('/') ? `${pathname}index.html` : pathname}`);
    if (!requestedPath.startsWith(root + sep)) throw new Error('Outside web root');
    const path = await realpath(requestedPath);
    if (!path.startsWith(root + sep) || !(await stat(path)).isFile()) throw new Error('Not a static file');
    const contents = await readFile(path);
    response.writeHead(200, { 'Content-Type': mime[extname(path)] ?? 'application/octet-stream', 'Content-Length': contents.length });
    response.end(request.method === 'HEAD' ? undefined : contents);
  } catch {
    response.writeHead(404, { 'Content-Type': 'text/plain; charset=utf-8' });
    response.end('Not found.');
  }
});
server.on('error', error => { console.error(error.message); process.exitCode = 1; });
server.listen(port, '127.0.0.1', () => console.log(`Optical Code: http://localhost:${port}/ (loopback only; static files only)`));
