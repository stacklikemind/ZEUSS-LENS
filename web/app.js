import { WasmClient, generateWithRetries } from './engine.mjs';

const $ = selector => document.querySelector(selector);
const form = $('#generate-form');
const fields = form.elements;
const downloadButtons = ['#download-svg', '#download-png', '#download-json'].map($);
let engine;
let latest;
let previewURL;
let busy = false;
let dirty = false;
let supportedPairs = [];
let profile = null;

function updateGenerateButton() {
  const approximate = profile?.needs_approximation && fields.allow_approximation.checked;
  $('#generate-button').disabled = busy || !engine || !profile?.can_generate || (profile.needs_approximation && !approximate);
  $('#generate-button span').textContent = busy ? 'Generating locally…' : approximate ? 'Generate approximate code' : 'Generate code';
}
function reviewProfile() {
  if (!engine) return;
  try {
    const response = engine.process({op:'review', right:eyeRequest('right'), left:eyeRequest('left'), axis_policy:fields.axis_policy.value});
    if (!response.ok) throw new Error(response.error?.message || 'Profile check failed.');
    profile = response.result;
    $('#profile-status').textContent = !profile.can_generate ? 'Check values' : profile.needs_approximation ? 'Approximation available' : 'Values fit the format';
    $('#profile-explanation').textContent = !profile.can_generate ? 'Review the values below before generating a code.' : profile.needs_approximation ? 'Your full prescription does not fit the code table. This optional profile keeps your sphere and reduces cylinder magnitude to the nearest encodable value. It is an experimental starting point.' : 'Your sphere and cylinder fit the code table. Any axis rounding is shown below.';
    $('#profile-review').classList.toggle('approximate', profile.needs_approximation);
    $('#profile-values').replaceChildren(); $('#profile-calibration-values').replaceChildren();
    for (const [side, label] of [['right','Right · OD'],['left','Left · OS']]) {
      const eye = profile.eyes[side];
      const card = document.createElement('div'); card.className = 'profile-eye';
      const title = document.createElement('h4'); title.textContent = label; card.append(title);
      if (eye.error) {const p = document.createElement('p'); p.textContent = eye.error; card.append(p);}
      else if (!eye.entered) card.append('No lens present');
      else {
        const list = document.createElement('dl');
        for (const [field,name] of [['entered','Your inserts'],['encoded','In the code']]) {
          const row = document.createElement('div'), term = document.createElement('dt'), value = document.createElement('dd');
          term.textContent = name; value.textContent = `${diopters(eye[field].sphere)} / ${diopters(eye[field].cylinder)} / ${axis(eye[field].axis)}`;
          row.append(term,value); list.append(row);
        }
        card.append(list);
        const calibration = document.createElement('p');
        calibration.textContent = eye.calibration?.available ? `${label}: ${diopters(eye.calibration.sphere)} / ${diopters(eye.calibration.cylinder)} D` : `${label}: nominal calibration unavailable for this pair.`;
        $('#profile-calibration-values').append(calibration);
      }
      $('#profile-values').append(card);
    }
    $('#profile-calibration').hidden = !$('#profile-calibration-values').children.length;
  } catch (error) {
    profile = null;
    $('#profile-status').textContent = 'Check values';
    $('#profile-explanation').textContent = error.message;
    $('#profile-values').replaceChildren(); $('#profile-calibration').hidden = true;
    $('#profile-review').classList.remove('approximate');
  }
  $('#approximation-choice').hidden = $('#approximation-help').hidden = !profile?.can_generate || !profile?.needs_approximation;
  updateGenerateButton();
}

function updateOpticalLimits() {
  if (!supportedPairs.length) return;
  for (const side of ['right', 'left']) {
    const sphere = fields[`${side}_sphere`];
    const cylinder = fields[`${side}_cylinder`];
    const values = [sphere.valueAsNumber, cylinder.valueAsNumber];
    for (const [field, input, other] of [
      ['sphere', sphere, values[1]], ['cylinder', cylinder, values[0]],
    ]) {
      const index = field === 'sphere' ? 0 : 1;
      const allowed = supportedPairs
        .filter(pair => !Number.isFinite(other) || pair[1 - index] === other)
        .map(pair => pair[index]);
      if (allowed.length) {
        input.removeAttribute('min'); input.removeAttribute('max');
        $(`#${side}-${field}-range`).textContent =
          `Exact code table: ${Math.min(...allowed)} to ${Math.max(...allowed)} D; 0.25 D steps with gaps.`;
      } else {
        input.removeAttribute('min');
        input.removeAttribute('max');
        $(`#${side}-${field}-range`).textContent = 'No code-table values for the other field.';
      }
    }
    cylinder.setCustomValidity('');
  }
}

function announce(message) { $('#live-status').textContent = message; }
function setBadge(message, kind = '') { const badge = $('#preview-badge'); badge.textContent = message; badge.className = `preview-badge ${kind}`; }
function hideError() { $('#form-error').hidden = true; $('#form-error').replaceChildren(); }
function showError(code, message) {
  const box = $('#form-error');
  box.replaceChildren();
  const title = document.createElement('strong');
  title.textContent = code === 'TOPOLOGY' ? 'This identity could not form a valid symbol.' : code === 'INPUT' ? 'Check the entered values.' : 'Generation could not finish.';
  box.append(title, document.createTextNode(message));
  box.hidden = false;
  announce(`${title.textContent} ${message}`);
}
function syncPresence() {
  for (const side of ['right', 'left']) {
    const present = fields[`${side}_present`].checked;
    $(`#${side}-eye`).classList.toggle('absent', !present);
    for (const field of ['sphere', 'cylinder', 'axis', 'offset']) fields[`${side}_${field}`].disabled = !present;
  }
  const fixed = fields.fixed_identity.checked;
  fields.secret_hex.disabled = !fixed;
  fields.secret_hex.required = fixed;
}
function markDirty(event) {
  if (event?.target !== fields.allow_approximation) fields.allow_approximation.checked = false;
  syncPresence();
  updateOpticalLimits();
  hideError();
  dirty = true;
  latest = null;
  if (previewURL) { URL.revokeObjectURL(previewURL); previewURL = null; }
  $('#symbol-image').hidden = true; $('#preview-placeholder').hidden = false;
  $('#report-content').hidden = true; $('#report-placeholder').hidden = false;
  setBadge('Inputs changed', 'stale'); $('#report-status').textContent = 'Generate to review this code';
  downloadButtons.forEach(button => { button.disabled = true; });
  reviewProfile();
}
form.addEventListener('input', markDirty);
form.addEventListener('change', markDirty);

function numberField(name) {
  const input = fields[name];
  if (input.value.trim() === '' || !Number.isFinite(input.valueAsNumber)) throw new Error(`Enter a finite value for ${name.replaceAll('_', ' ')}.`);
  return input.valueAsNumber;
}
function eyeRequest(side) {
  if (!fields[`${side}_present`].checked) return null;
  return { sphere: numberField(`${side}_sphere`), cylinder: numberField(`${side}_cylinder`), axis: numberField(`${side}_axis`), rx_offset_id: Number(fields[`${side}_offset`].value) };
}
function diopters(value) {
  if (typeof value !== 'number' || !Number.isFinite(value)) return '—';
  return `${value > 0 ? '+' : ''}${value.toFixed(2)}`;
}
function axis(value) { return typeof value === 'number' ? `${value}°` : '—'; }
function addCell(row, text, className) {
  const cell = document.createElement('td');
  cell.textContent = text;
  if (className) cell.className = className;
  row.append(cell);
  return cell;
}
function renderComparisons(result) {
  const body = $('#comparison-body');
  body.replaceChildren();
  for (const [side, label] of [['right', 'Right · OD'], ['left', 'Left · OS']]) {
    const entered = result.entered?.[side];
    const encoded = result.encoded?.[side];
    if (!entered || !encoded || encoded.lens_present === false) {
      const row = document.createElement('tr');
      addCell(row, label);
      const absent = addCell(row, 'Absent · no lens encoded', 'row-label');
      absent.colSpan = 5;
      body.append(row);
      continue;
    }
    const entryRow = document.createElement('tr');
    entryRow.className = 'entered-row';
    addCell(entryRow, label).rowSpan = 2;
    addCell(entryRow, 'Entered', 'row-label');
    addCell(entryRow, diopters(entered.sphere));
    addCell(entryRow, diopters(entered.cylinder));
    addCell(entryRow, axis(entered.axis));
    addCell(entryRow, `${diopters((entered.rx_offset_id ?? 0) / 4)} D`);
    const encodedRow = document.createElement('tr');
    encodedRow.className = 'encoded-row';
    addCell(encodedRow, 'Encoded', 'row-label');
    for (const field of ['sphere', 'cylinder', 'axis']) {
      const text = field === 'axis' ? axis(encoded[field]) : diopters(encoded[field]);
      const cell = addCell(encodedRow, '');
      const value = document.createElement('span');
      value.textContent = text;
      if (entered[field] !== encoded[field]) value.className = 'changed-value';
      cell.append(value);
    }
    addCell(encodedRow, `${diopters((encoded.rx_offset_id ?? 0) / 4)} D`);
    body.append(entryRow, encodedRow);
  }
}
function renderWarnings(result) {
  const container = $('#warnings');
  container.replaceChildren();
  for (const warning of result.warnings ?? []) {
    const item = document.createElement('div');
    item.className = 'message warning-message';
    item.textContent = typeof warning === 'string' ? warning : JSON.stringify(warning);
    container.append(item);
  }
  for (const [side, name] of [['right', 'Right eye'], ['left', 'Left eye']]) {
    const entered = result.entered?.[side], encoded = result.encoded?.[side];
    if (entered && encoded && entered.axis !== encoded.axis && Math.abs(entered.axis - encoded.axis) % 180 === 0) {
      const item = document.createElement('div');
      item.className = 'message warning-message';
      item.textContent = `${name}: ${entered.axis}° is encoded as ${encoded.axis}°. These are equivalent axis representations.`;
      container.append(item);
    }
  }
}
function renderCalibration(result) {
  const container = $('#calibration-values');
  container.replaceChildren();
  for (const [side, name] of [['right', 'Right'], ['left', 'Left']]) {
    const calibration = result.calibration?.[side];
    const line = document.createElement('p');
    line.className = 'calibration-line';
    const label = document.createElement('strong');
    label.textContent = `${name}: `;
    line.append(label);
    if (!result.encoded?.[side]) {
      line.append('Absent.');
    } else if (!calibration || calibration.available === false) {
      line.classList.add('unavailable');
      line.append(`Unavailable${calibration?.reason ? ` — ${calibration.reason}` : '.'}`);
    } else {
      const canonical = result.encoded[side].canonical;
      const changed = calibration.changed || (canonical && (canonical.sphere !== calibration.sphere || canonical.cylinder !== calibration.cylinder));
      if (changed) line.classList.add('changed');
      line.append(`S ${diopters(calibration.sphere)} D / C ${diopters(calibration.cylinder)} D`);
      if (changed) line.append(` · adjusted${calibration.clamped ? ' and clamped to calibration limits' : ''}`);
      else line.append(' · unchanged');
      if (calibration.offset_diopters) line.append(` · supplied add ${diopters(calibration.offset_diopters)} D`);
    }
    container.append(line);
  }
}
function renderValidation(validation = {}) {
  const container = $('#validation-summary');
  container.replaceChildren();
  const zeroSyndromes = ['metadata_syndromes', 'gap_syndromes', 'arc_syndromes'].every(key => Array.isArray(validation[key]) && validation[key].length === 2 && validation[key].every(value => value === 0));
  const checks = [
    ['Payload bit round trip', validation.payload_roundtrip === true],
    ['All three RS syndrome pairs', zeroSyndromes],
    ['Gap & arc checksums', validation.gap_checksum === true && validation.arc_checksum === true],
    ['Redundant arc colors', validation.redundant_colors === true],
  ];
  for (const [label, passed] of checks) {
    const row = document.createElement('div');
    row.className = 'validation-item';
    const mark = document.createElement('span');
    mark.className = 'check-icon';
    mark.textContent = passed ? '✓' : '–';
    mark.setAttribute('aria-hidden', 'true');
    row.append(mark, document.createTextNode(`${label} · ${passed ? 'passed' : 'not confirmed'}`));
    container.append(row);
  }
}
function reportDocument(run) {
  return {
    format_studio: { abi_version: 1, generated_at: new Date().toISOString(), identity_mode: run.identity_mode, identity_attempts: run.attempts, verification_scope: 'In-memory symbol bits only. No raster, native runtime, camera, or hardware verification.', synthetic_identity: true },
    request: run.request,
    ...run.response,
  };
}
function renderResult(run) {
  const result = run.response.result;
  if (typeof result?.svg !== 'string' || typeof result?.payload_hex !== 'string') throw new Error('The local engine returned an incomplete generation result.');
  const url = URL.createObjectURL(new Blob([result.svg], { type: 'image/svg+xml' }));
  const image = $('#symbol-image');
  image.src = url;
  image.alt = 'Generated circular optical code for the current entered lens values';
  if (previewURL) URL.revokeObjectURL(previewURL);
  previewURL = url;
  image.hidden = false;
  $('#preview-placeholder').hidden = true;
  setBadge(result.profile_adjustments?.length ? 'Approximate · generated locally' : 'Generated locally', 'generated');
  $('#symbol-caption').textContent = `880 × 880 · ${result.colors?.length ?? '—'} visible arcs`;
  $('#attempt-caption').textContent = run.identity_mode === 'fixed' ? 'Fixed identity · 1 attempt' : `Fresh identity · ${run.attempts} ${run.attempts === 1 ? 'attempt' : 'attempts'}`;
  renderWarnings(result);
  renderComparisons(result);
  renderCalibration(result);
  renderValidation(result.validation);
  const report = reportDocument(run);
  $('#payload-hex').textContent = result.payload_hex;
  $('#json-report').textContent = JSON.stringify(report, null, 2);
  $('#report-placeholder').hidden = true;
  $('#report-content').hidden = false;
  $('#report-status').textContent = 'Current result · inspect adjustments below';
  latest = { ...run, report };
  dirty = false;
  downloadButtons.forEach(button => { button.disabled = false; });
  announce(`Code generated locally after ${run.attempts} ${run.attempts === 1 ? 'identity attempt' : 'identity attempts'}. Review encoded values and calibration adjustments below.`);
}
async function generate(event) {
  event?.preventDefault();
  if (!engine || busy || !profile?.can_generate || (profile.needs_approximation && !fields.allow_approximation.checked) || !form.reportValidity()) return;
  hideError();
  busy = true;
  $('#generate-button').disabled = true;
  $('#generate-button span').textContent = 'Generating locally…';
  downloadButtons.forEach(button => { button.disabled = true; });
  try {
    // Yield for the busy state before reading inputs. Capture and process remain synchronous.
    await new Promise(resolve => requestAnimationFrame(resolve));
    if (!form.reportValidity()) return;
    const optics = { op: 'generate', right: eyeRequest('right'), left: eyeRequest('left'), axis_policy: fields.axis_policy.value, diopter_policy:fields.allow_approximation.checked ? 'reduce_cylinder' : 'exact', lens_type: Number(fields.lens_type.value) };
    if (!optics.left && !optics.right) throw new Error('At least one eye must be present.');
    const fixedIdentity = fields.fixed_identity.checked ? fields.secret_hex.value.trim().toLowerCase() : null;
    if (fixedIdentity !== null && !/^[0-9a-f]{20}$/.test(fixedIdentity)) throw new Error('A fixed identity must contain exactly 20 hexadecimal characters.');
    const run = generateWithRetries(engine, optics, fixedIdentity);
    if (!run.response.ok) {
      const code = run.response.error?.code ?? 'ENGINE';
      let message = run.response.error?.message ?? 'The local engine did not accept the request.';
      if (code === 'TOPOLOGY') message += fixedIdentity === null ? ' Tried 64 fresh identities with the same optics. Generate again to try new identities.' : ' The fixed identity was kept unchanged. Choose another identity or return to fresh identity mode.';
      showError(code, message);
      setBadge(latest ? 'Previous result' : 'No result', latest ? 'stale' : '');
      $('#report-status').textContent = latest ? 'Previous result · generation failed' : 'No result';
      dirty = true;
      return;
    }
    renderResult(run);
  } catch (error) {
    showError('INPUT', error instanceof Error ? error.message : String(error));
    setBadge(latest ? 'Previous result' : 'No result', latest ? 'stale' : '');
    dirty = true;
  } finally {
    busy = false;
    updateGenerateButton();
  }
}
form.addEventListener('submit', generate);

function download(blob, extension) {
  const link = document.createElement('a');
  const url = URL.createObjectURL(blob);
  link.href = url;
  link.download = `optical-code-${latest.response.result.payload_hex.slice(0, 12)}.${extension}`;
  document.body.append(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10000);
}
$('#download-svg').addEventListener('click', () => {
  if (latest && !dirty && !busy) download(new Blob([latest.response.result.svg], { type: 'image/svg+xml;charset=utf-8' }), 'svg');
});
$('#download-json').addEventListener('click', () => {
  if (latest && !dirty && !busy) download(new Blob([JSON.stringify(latest.report, null, 2) + '\n'], { type: 'application/json' }), 'json');
});
$('#download-png').addEventListener('click', async () => {
  if (!latest || dirty || busy) return;
  const current = latest;
  const button = $('#download-png');
  button.disabled = true;
  let url;
  try {
    url = URL.createObjectURL(new Blob([current.response.result.svg], { type: 'image/svg+xml' }));
    const image = new Image();
    image.src = url;
    await image.decode();
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 1760;
    const context = canvas.getContext('2d');
    if (!context) throw new Error('This browser cannot create a PNG canvas.');
    context.fillStyle = '#ffffff';
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    const blob = await new Promise(resolve => canvas.toBlob(resolve, 'image/png'));
    if (!blob) throw new Error('This browser could not export the PNG.');
    if (current === latest && !dirty && !busy) { download(blob, 'png'); announce('Downloaded PNG at 1760 × 1760 pixels with a white background.'); }
  } catch (error) {
    showError('EXPORT', error instanceof Error ? error.message : String(error));
  } finally {
    if (url) URL.revokeObjectURL(url);
    button.disabled = dirty || busy || !latest;
  }
});
$('#copy-payload').addEventListener('click', async () => {
  if (!latest) return;
  try {
    await navigator.clipboard.writeText(latest.response.result.payload_hex);
    $('#copy-payload').textContent = 'Copied';
    setTimeout(() => { $('#copy-payload').textContent = 'Copy'; }, 1500);
  } catch { announce('Copy is unavailable. Select the payload text to copy it manually.'); }
});

syncPresence();
try {
  engine = await WasmClient.load();
  const config = engine.process({ op: 'config' });
  if (!config.ok) throw new Error('Could not load supported optical values.');
  supportedPairs = config.result.supported_pairs.flatMap(pair => {
    const sphere = pair.sphere_cd / 100;
    const cylinder = pair.cylinder_cd / 100;
    return [[sphere, cylinder], [sphere + cylinder, -cylinder]];
  });
  updateOpticalLimits();
  reviewProfile();
  $('#engine-status').classList.add('ready');
  $('#engine-status').lastChild.textContent = 'Local engine ready';
  updateGenerateButton();
  await generate();
} catch (error) {
  $('#engine-status').classList.add('failed');
  $('#engine-status').lastChild.textContent = 'Engine unavailable';
  setBadge('Engine unavailable');
  showError('ENGINE', error instanceof Error ? error.message : String(error));
}
