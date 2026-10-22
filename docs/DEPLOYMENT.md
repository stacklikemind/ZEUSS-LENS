# Cloudflare Pages deployment

The application is a static Cloudflare Pages site. Cloudflare serves HTML, CSS, JavaScript and `vision_code.wasm`; code generation takes place in the visitor's browser. There are no Pages Functions, server-side generation endpoints, user-input storage or API secrets in the application.

## Live deployment

- Primary GPUI app: https://zeuss-lens.pages.dev/
- Alternate GPUI path: https://zeuss-lens.pages.dev/gpui/
- Lightweight frontend: https://zeuss-lens.pages.dev/lightweight.html
- Encoder-only module: https://zeuss-lens.pages.dev/vision_code.wasm
- Production deployment: https://0d75cbdb.zeuss-lens.pages.dev/

Deployed on 22 October 2026 from source commit `8cee1ddbe0a5047187788aeadaf1ab27c26ca779`. GPUI WASM SHA-256: `1897ad7f9c3fb492a47a2b85bfe4d9286621f8ba5ef541a2fa48e42e78325abf`. Encoder-only WASM SHA-256: `40da098b570a1045797a0169cacd6e07656551636216576d520c347bde7188af`.

Production root HTML, both GPUI WASM paths, encoder-only WASM, lightweight HTML and its JavaScript were downloaded and matched the local build byte for byte. The live GPUI app also displayed the optional −11.00 / −1.75 profile for an entered −11.00 / −3.75 prescription and generated a code after explicit selection. This verifies deployed behavior, not physical headset acceptance or tracking quality.

The former Pages address redirects to this project.

## Rebuild and deploy

Use Rust with the `wasm32-unknown-unknown` target, and Node.js 22 or newer for Wrangler. Authentication is managed by Wrangler outside this repository.

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129 --locked
npx --yes wrangler@4.151.1 login
./scripts/deploy.sh
```

The script runs the Rust tests, builds WASM, tests the browser adapter, builds the GPUI WASM frontend, and uploads `web/` to the `zeuss-lens` Pages project on its production branch `main`. It uses Wrangler 4.151.1 explicitly. The Git repository excludes build outputs and credential/config-cache files.

For a new Cloudflare account, create the project first:

```sh
npx --yes wrangler@4.151.1 pages project create zeuss-lens --production-branch main --force
```

Wrangler 4.151.1 delegates new project creation to Workers by default; `--force` selects Pages Direct Upload for this initial creation. Do not add it to subsequent deploy commands for the existing project.

For multiple accounts, set `CLOUDFLARE_ACCOUNT_ID` in your local environment. Never commit account tokens. Only the contents of `web/` are deployed; source, fixtures, analysis records and local files are not part of the hosted site's assets.

## GitHub builds

The included GitHub Actions workflow runs tests and builds a `cloudflare-pages-assets` artifact on pushes to `main`, pull requests and manual dispatch. It does not require or store Cloudflare credentials, and does not automatically deploy. Production deployment uses the explicit script above.

This project uses Pages Direct Upload, rather than Cloudflare's Git integration. Cloudflare does not support converting an existing Direct Upload project to Git integration; a future switch requires another Pages project. Direct Upload can also be driven by a separately configured CI deployment workflow.

## Browser delivery

`web/_headers` supplies the same content policy used locally, including WebAssembly execution support and same-origin application requests. It also supplies the WASM media type, permits cross-origin downloads of the public WASM module, and requests revalidation so a stable asset URL does not retain an obsolete module after deployment.

Hosted requests for public assets are handled by Cloudflare. Prescription fields and synthetic identities are processed in the browser and are not sent to Cloudflare by the app.

References: [Cloudflare Direct Upload](https://developers.cloudflare.com/pages/get-started/direct-upload/), [Wrangler Pages commands](https://developers.cloudflare.com/workers/wrangler/commands/pages/), and [Pages response headers](https://developers.cloudflare.com/pages/configuration/headers/).
