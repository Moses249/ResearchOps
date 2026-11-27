const $ = id => document.getElementById(id);
let active = null;
let busy = false;
let connected = false;
let selectedRun = null;
let contractOptions = [];

function node(tag, cls, text) {
  const element = document.createElement(tag);
  if (cls) element.className = cls;
  if (text !== undefined) element.textContent = text;
  return element;
}

async function api(url, body, method = 'POST') {
  const options = body === undefined ? {} : {
    method, headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body)
  };
  const response = await fetch(url, options);
  const text = await response.text();
  let value;
  try { value = JSON.parse(text); } catch { value = { error: text }; }
  if (!response.ok) throw Error(value.error || `Request failed (${response.status})`);
  return value;
}

function setBusy(value) {
  busy = value;
  for (const id of ['create', 'example', 'add-source', 'refresh', 'contract-picker', 'inspect-recovery']) $(id).disabled = value;
  $('agent-mode').disabled = value || !connected;
  for (const id of ['resume', 'retry']) if ($(id)) $(id).disabled = value;
  if (!value && selectedRun) render(selectedRun);
}

function addSource(data = {}) {
  const card = node('div', 'source-edit');
  const top = node('div', 'source-top');
  top.append(node('span', '', 'DOCUMENT SNAPSHOT'));
  const remove = node('button', 'text-btn', 'Remove');
  remove.type = 'button';
  remove.onclick = () => card.remove();
  top.append(remove);
  card.append(top);
  for (const [key, label, placeholder] of [
    ['title', 'Title', 'Document title'],
    ['uri', 'Source reference (optional)', 'Stored reference — never fetched'],
    ['content', 'Document text', 'Paste source text here…']
  ]) {
    const field = node('label', 'field');
    field.append(node('span', '', label));
    const input = node(key === 'content' ? 'textarea' : 'input');
    input.dataset.key = key;
    input.placeholder = placeholder;
    input.value = data[key] || '';
    input.required = key !== 'uri';
    if (key === 'title') input.maxLength = 500;
    if (key === 'uri') input.maxLength = 2000;
    field.append(input);
    card.append(field);
  }
  $('sources').append(card);
}

function getSpec() {
  const sources = [...$('sources').children].map(card => Object.fromEntries(
    [...card.querySelectorAll('[data-key]')].map(input => [input.dataset.key, input.value])
  ));
  if (!sources.length) throw Error('Add at least one source snapshot.');
  return {
    objective: $('objective').value,
    questions: $('questions').value.split('\n').map(q => q.trim()).filter(Boolean), sources
  };
}

async function refresh() {
  const runs = await api('/api/runs');
  $('runs').replaceChildren();
  for (const run of runs) {
    const button = node('button', 'btn run-tab', run.objective);
    button.setAttribute('aria-pressed', String(run.id === active));
    button.title = run.objective;
    button.disabled = busy;
    button.onclick = () => selectRun(run.id).catch(showError);
    $('runs').append(button);
  }
  return runs;
}

function showError(error) { $('workspace-error').textContent = error.message; }

async function selectRun(id) {
  active = id;
  $('workspace-error').textContent = '';
  const run = await api('/api/runs/' + id);
  render(run);
  await refresh();
  if (run.agent_active && !busy) await pollRun(id);
}

function render(run) {
  selectedRun = run;
  const root = $('workspace');
  root.replaceChildren();
  const body = node('div', 'panel-body');
  const execution = run.execution;
  const business = execution ? execution.business_status : run.status;
  body.append(node('span', 'status ' + (business === 'COMPLETE' ? 'complete' : 'partial'), run.agent_active ? 'executing' : business),
    node('h2', 'run-title', run.objective), node('div', 'run-id', run.id));
  const stats = node('div', 'stats');
  for (const [value, label] of [
    [`${run.processed_questions}/${run.total_questions}`, 'Questions processed'],
    [`${run.covered_questions}/${run.total_questions}`, 'With evidence'],
    [String(run.total_questions - run.covered_questions), 'Coverage gaps']
  ]) {
    const stat = node('div', 'stat');
    stat.append(node('strong', '', value), node('span', '', label));
    stats.append(stat);
  }
  body.append(stats);
  if (execution) {
    const session = execution.latest;
    body.append(node('div', 'execution-state', `System: ${session ? session.system_status : 'no Runner session'} · Business: ${business} · Scope: ${execution.completion_scope}`));
    if (session) {
      body.append(node('small', '', `Trigger: ${session.trigger} · KST: ${session.start_kst_date}${session.end_kst_date ? ' → ' + session.end_kst_date : ''} · Elapsed: ${session.elapsed_ms} ms / ${session.budget_seconds}s`));
      if (session.stop_code) body.append(node('small', '', `Stop: ${session.stop_code} · Last success: ${session.last_successful_stage || 'none recorded'} · Failure stage: ${session.failure_stage || 'none'}`));
      if (session.error) body.append(node('div', 'error', session.error));
      if (session.before && session.after) body.append(node('small', '', `Processed: ${session.before.succeeded} → ${session.after.succeeded} · Readbacks: ${session.before.readback_verified} → ${session.after.readback_verified}`));
    }
    if (execution.gate_failures.length) {
      const gates = node('div', 'gate-list');
      gates.append(node('strong', '', 'Completion gates still open'));
      const list = node('ul');
      execution.gate_failures.forEach(gate => list.append(node('li', '', gate)));
      gates.append(list);
      body.append(gates);
    }
    const resumeDetails = node('details', 'activity');
    resumeDetails.append(node('summary', '', 'SQL resume position'));
    resumeDetails.append(node('pre', 'resume-sql', JSON.stringify(execution.resume, null, 2)));
    body.append(resumeDetails);
  }
  const actions = node('div', 'actions');
  const resume = node('button', 'btn primary', 'Process / resume');
  resume.id = 'resume';
  resume.disabled = busy || run.agent_active || business === 'COMPLETE';
  resume.onclick = () => processRun(run.id, run.agent_events.length > 0 || execution?.latest?.mode === 'agents').catch(showError);
  const report = node('a', 'btn', '↓ Export Markdown');
  report.href = '/api/runs/' + run.id + '/report';
  actions.append(resume, report);
  const recoveryLink = node('a', 'btn', 'Recovery JSON');
  recoveryLink.href = '/api/runs/' + run.id + '/recovery';
  recoveryLink.download = 'researchops-recovery.json';
  actions.append(recoveryLink);
  if (run.tasks.some(task => task.status === 'failed' && task.attempts < 3)) {
    const retry = node('button', 'btn', 'Retry failed tasks');
    retry.id = 'retry';
    retry.disabled = busy || run.agent_active;
    retry.onclick = async () => {
      try {
        await api('/api/runs/' + run.id + '/retry', {});
        await processRun(run.id, run.agent_events.length > 0 || execution?.latest?.mode === 'agents');
      } catch (error) { showError(error); }
    };
    actions.append(retry);
  }
  body.append(actions);
  body.append(node('div', 'notice', 'Exact quotes establish provenance. Model-reviewed answers pass a separate verifier conversation; they can still be wrong. Coverage is limited to supplied documents.'));
  if (run.agent_error) body.append(node('div', 'error', run.agent_error));
  if (run.agent_events.length) {
    const activity = node('details', 'activity');
    activity.open = true;
    activity.append(node('summary', '', `Agent activity · ${run.agent_events.length} events`));
    const events = node('div', 'event-list');
    for (const event of run.agent_events.slice(-60)) {
      const row = node('div', 'event-row');
      const question = run.tasks.findIndex(task => task.id === event.task_id) + 1;
      row.append(node('strong', '', `Q${question} · ${event.role} · round ${event.round} · ${event.status}`),
        node('div', '', event.message));
      events.append(row);
    }
    activity.append(events);
    body.append(activity);
  }
  root.append(body);
  for (const [index, task] of run.tasks.entries()) {
    const section = node('section', 'question');
    const head = node('div', 'q-head');
    head.append(node('h3', '', `${String(index + 1).padStart(2, '0')}  ${task.question}`),
      node('span', 'status ' + task.status, task.status));
    section.append(head, node('small', '', `Deliveries: ${task.attempts} / 3`));
    if (task.answer) section.append(node('div', 'answer', task.answer), node('small', '', 'Model-reviewed answer · inspect the evidence below'));
    if (task.last_error) section.append(node('div', 'error', task.last_error));
    if (!task.findings.length) section.append(node('div', 'gap',
      task.status === 'succeeded' ? 'No supporting evidence recorded. Add sources or refine the question.' :
        task.status === 'failed' ? 'Processing failed. Inspect the review history before retrying.' : 'Waiting for evidence.'));
    for (const finding of task.findings) {
      section.append(node('blockquote', 'quote', finding.quote), node('div', 'source-meta',
        `${finding.source_title} · ${finding.verification}${task.answer ? ' · reviewed by verifier' : ` · ${Math.round(finding.score * 100)}% keyword overlap`}`));
      if (finding.source_uri) section.append(node('div', 'source-meta', finding.source_uri));
      section.append(node('div', 'source-meta hash', 'SHA-256 ' + finding.source_hash));
    }
    root.append(section);
  }
}

async function pollRun(id) {
  setBusy(true);
  try {
    while (true) {
      const run = await api('/api/runs/' + id);
      render(run);
      if (!run.agent_active) break;
      await new Promise(resolve => setTimeout(resolve, 1000));
    }
  } finally { setBusy(false); await refresh(); }
}

async function processRun(id, forceAgents = false) {
  if (busy) return;
  const knownMode = selectedRun?.id === id ? selectedRun.execution?.latest?.mode : null;
  const useAgents = knownMode ? knownMode === 'agents' : forceAgents || $('agent-mode').checked;
  if (useAgents && !connected) {
    $('connection-dialog').showModal();
    throw Error('Connect and test your API before starting agent mode.');
  }
  setBusy(true);
  $('workspace-error').textContent = '';
  try {
    await api('/api/runs/' + id + '/execute', {mode: useAgents ? 'agents' : 'local', trigger: 'manual'});
    await pollRun(id);
  } finally { setBusy(false); await refresh(); }
}

function connectionStatus(value) {
  connected = value.configured && value.tested;
  $('open-settings').textContent = connected ? '● API connected' : value.configured ? '○ Test API connection' : '○ Connect API';
  $('connection-state').textContent = connected ? 'Connected · both agent models tested' : value.configured ? 'Saved in this server session. Test both models to enable agents.' : 'No API connected. Local research works without a key.';
  $('agent-mode').disabled = !connected || busy;
  if (!connected) $('agent-mode').checked = false;
  $('test-connection').disabled = !value.configured;
}

$('open-settings').onclick = async () => {
  $('connection-error').textContent = '';
  try {
    const connection = await api('/api/connection');
    connectionStatus(connection);
    if (connection.configured) {
      $('base-url').value = connection.base_url;
      $('researcher-model').value = connection.researcher_model;
      $('verifier-model').value = connection.verifier_model;
    }
    $('api-key').value = '';
    $('connection-dialog').showModal();
  } catch (error) { showError(error); }
};
$('close-settings').onclick = () => $('connection-dialog').close();
$('connection-form').onsubmit = async event => {
  event.preventDefault();
  $('connection-error').textContent = '';
  $('save-connection').disabled = true;
  try {
    const connection = await api('/api/connection', {
      base_url: $('base-url').value, researcher_model: $('researcher-model').value,
      verifier_model: $('verifier-model').value, api_key: $('api-key').value
    });
    $('api-key').value = '';
    connectionStatus(connection);
  } catch (error) { $('connection-error').textContent = error.message; }
  finally { $('save-connection').disabled = false; }
};
$('test-connection').onclick = async () => {
  $('test-connection').disabled = true;
  $('connection-error').textContent = '';
  $('connection-state').textContent = 'Testing Researcher and Verifier models…';
  try { connectionStatus(await api('/api/connection/test', {})); }
  catch (error) {
    $('connection-error').textContent = error.message;
    $('connection-state').textContent = 'Test failed. Check endpoint, model names and API key.';
  } finally { $('test-connection').disabled = false; }
};
$('disconnect').onclick = async () => {
  try { connectionStatus(await api('/api/connection', {}, 'DELETE')); }
  catch (error) { $('connection-error').textContent = error.message; }
};
$('research-form').onsubmit = async event => {
  event.preventDefault();
  if (busy) return;
  $('form-error').textContent = '';
  setBusy(true);
  let id;
  try {
    const result = await api('/api/runs', getSpec());
    id = result.id;
    const contract = contractOptions.find(value => value.version === $('contract-picker').value);
    if (contract) await api('/api/runs/' + id + '/contract', {version: contract.version, expected_md5: contract.md5});
    await selectRun(id);
  } catch (error) {
    id = undefined;
    $('form-error').textContent = error.message;
  }
  finally { setBusy(false); }
  if (id) await processRun(id).catch(showError);
};
$('example').onclick = async () => {
  try {
    const example = await api('/api/example');
    $('objective').value = example.objective;
    $('questions').value = example.questions.join('\n');
    $('sources').replaceChildren();
    example.sources.forEach(addSource);
  } catch (error) { $('form-error').textContent = error.message; }
};
$('add-source').onclick = () => addSource();
$('refresh').onclick = () => (active ? selectRun(active) : refresh()).catch(showError);
addSource();
api('/api/contracts').then(contracts => {
  contractOptions = contracts;
  $('contract-picker').replaceChildren();
  for (const contract of contracts) {
    const policy = JSON.parse(contract.body);
    const option = node('option', '', `${contract.version} · ${policy.max_runtime_seconds}s · batch ${policy.batch_size}`);
    option.value = contract.version;
    option.selected = contract.version === 'researchops-evidence-v1';
    $('contract-picker').append(option);
  }
}).catch(showError);
$('inspect-recovery').onclick = async () => {
  try {
    const result = await api('/api/recovery');
    const panel = $('recovery-panel');
    panel.replaceChildren(node('h3', '', 'Unfinished workflows'));
    if (result.database_error) panel.append(node('div', 'error', result.database_error));
    for (const candidate of result.runs) {
      const entry = node('div', 'recovery-entry');
      const open = node('button', 'text-btn', candidate.run_id);
      open.onclick = () => selectRun(candidate.run_id).catch(showError);
      entry.append(open, node('small', '', `${candidate.execution.business_status} · ${candidate.execution.gate_failures.join(', ')}`));
      panel.append(entry);
    }
    panel.append(node('small', '', `${result.files.length} local recovery file(s). Selecting a run does not start it.`));
    if (!result.runs.length && !result.database_error) panel.append(node('p', '', 'No unfinished workflows.'));
  } catch (error) { showError(error); }
};
api('/api/connection').then(connectionStatus).catch(showError);
refresh().then(runs => { if (runs.length) return selectRun(runs[0].id); }).catch(showError);
