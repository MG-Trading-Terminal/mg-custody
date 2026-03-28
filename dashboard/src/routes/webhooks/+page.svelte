<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface WebhookEndpoint {
		id: string;
		url: string;
		enabled: boolean;
		description: string | null;
		event_filter: string[];
		chain_filter: string[];
		success_count: number;
		failure_count: number;
		success_rate_pct: number;
		avg_latency_ms: number | null;
		last_delivery_at: string | null;
		created_at: string;
	}

	interface Delivery {
		id: string;
		event_type: string;
		attempts: number;
		status: string;
		last_error: string | null;
		created_at: string;
		delivered_at: string | null;
	}

	interface Metrics {
		total_deliveries: number;
		total_success: number;
		total_failure: number;
		overall_success_rate_pct: number;
	}

	let endpoints    = $state<WebhookEndpoint[]>([]);
	let metrics      = $state<Metrics | null>(null);
	let deliveries   = $state<Delivery[]>([]);
	let selectedId   = $state<string | null>(null);
	let loading      = $state(true);
	let showModal    = $state(false);
	let testResult   = $state<{ success: boolean; status_code?: number; error?: string } | null>(null);
	let testingId    = $state<string | null>(null);

	// Form state
	let formUrl         = $state('');
	let formDesc        = $state('');
	let formEnabled     = $state(true);
	let formEventFilter = $state('');
	let editId          = $state<string | null>(null);
	let formError       = $state('');

	async function load() {
		loading = true;
		const [epRes, mRes] = await Promise.allSettled([
			api.getJson<{ count: number; endpoints: WebhookEndpoint[] }>('/v1/admin/webhooks'),
			api.getJson<Metrics>('/v1/admin/webhooks/metrics'),
		]);
		endpoints = epRes.status === 'fulfilled' ? (epRes.value.endpoints ?? []) : [];
		metrics   = mRes.status  === 'fulfilled' ? mRes.value : null;
		loading   = false;
	}

	async function loadDeliveries(id: string) {
		selectedId = id;
		try {
			const res = await api.getJson<{ deliveries: Delivery[] }>(`/v1/admin/webhooks/${id}/deliveries`);
			deliveries = res.deliveries ?? [];
		} catch {
			deliveries = [];
		}
	}

	async function testWebhook(id: string) {
		testingId  = id;
		testResult = null;
		try {
			testResult = await api.post(`/v1/admin/webhooks/${id}/test`);
		} catch (e: unknown) {
			testResult = { success: false, error: String(e) };
		}
		testingId = null;
	}

	function openCreate() {
		editId = null;
		formUrl = ''; formDesc = ''; formEnabled = true; formEventFilter = '';
		formError = '';
		showModal = true;
	}

	function openEdit(ep: WebhookEndpoint) {
		editId          = ep.id;
		formUrl         = ep.url;
		formDesc        = ep.description ?? '';
		formEnabled     = ep.enabled;
		formEventFilter = ep.event_filter.join(', ');
		formError       = '';
		showModal       = true;
	}

	async function saveWebhook() {
		formError = '';
		if (!formUrl.startsWith('http')) {
			formError = 'URL must start with http:// or https://';
			return;
		}
		const body = {
			url: formUrl,
			description: formDesc || null,
			enabled: formEnabled,
			event_filter: formEventFilter
				? formEventFilter.split(',').map(s => s.trim()).filter(Boolean)
				: [],
			chain_filter: [],
		};
		try {
			if (editId) {
				await api.put(`/v1/admin/webhooks/${editId}`, body);
			} else {
				await api.post('/v1/admin/webhooks', body);
			}
			showModal = false;
			await load();
		} catch (e: unknown) {
			formError = String(e);
		}
	}

	async function deleteEndpoint(id: string) {
		if (!confirm('Delete this webhook endpoint?')) return;
		try {
			await api.del(`/v1/admin/webhooks/${id}`);
			if (selectedId === id) { selectedId = null; deliveries = []; }
			await load();
		} catch (e: unknown) {
			alert('Delete failed: ' + String(e));
		}
	}

	function successBadge(pct: number) {
		if (pct >= 95) return 'green';
		if (pct >= 80) return 'yellow';
		return 'red';
	}

	function statusBadge(s: string) {
		if (s === 'delivered') return 'green';
		if (s === 'failed')    return 'red';
		return 'yellow';
	}

	function fmt(ts: string | null) {
		if (!ts) return '—';
		return new Date(ts).toLocaleString();
	}

	onMount(load);
</script>

<h1>Webhook Management</h1>

{#if metrics}
<div class="metrics-bar">
	<div class="metric">
		<span class="label">Total Deliveries</span>
		<span class="val">{metrics.total_deliveries.toLocaleString()}</span>
	</div>
	<div class="metric">
		<span class="label">Success Rate</span>
		<span class="val {successBadge(metrics.overall_success_rate_pct)}">{metrics.overall_success_rate_pct.toFixed(1)}%</span>
	</div>
	<div class="metric">
		<span class="label">Success</span>
		<span class="val green">{metrics.total_success.toLocaleString()}</span>
	</div>
	<div class="metric">
		<span class="label">Failed</span>
		<span class="val {metrics.total_failure > 0 ? 'red' : 'muted'}">{metrics.total_failure.toLocaleString()}</span>
	</div>
</div>
{/if}

<section>
	<div class="section-header">
		<h2>Endpoints</h2>
		<button class="btn-primary" onclick={openCreate}>+ Add Endpoint</button>
	</div>
	<div class="card">
		{#if loading}
			<p class="muted">Loading…</p>
		{:else if endpoints.length === 0}
			<p class="muted">No webhook endpoints configured.</p>
		{:else}
			<div class="table-wrap"><table>
				<thead>
					<tr>
						<th>URL</th>
						<th>Status</th>
						<th>Success Rate</th>
						<th>Avg Latency</th>
						<th>Last Delivery</th>
						<th>Actions</th>
					</tr>
				</thead>
				<tbody>
					{#each endpoints as ep}
						<tr class={selectedId === ep.id ? 'selected' : ''}>
							<td>
								<div class="ep-url">{ep.url}</div>
								{#if ep.description}<div class="ep-desc muted">{ep.description}</div>{/if}
							</td>
							<td>
								<span class="badge {ep.enabled ? 'green' : 'red'}">
									{ep.enabled ? 'active' : 'paused'}
								</span>
							</td>
							<td>
								<span class="badge {successBadge(ep.success_rate_pct)}">
									{ep.success_rate_pct.toFixed(1)}%
								</span>
							</td>
							<td class="muted">{ep.avg_latency_ms != null ? `${Math.round(ep.avg_latency_ms)}ms` : '—'}</td>
							<td class="muted">{fmt(ep.last_delivery_at)}</td>
							<td class="actions">
								<button class="btn-sm" onclick={() => loadDeliveries(ep.id)}>Deliveries</button>
								<button
									class="btn-sm"
									onclick={() => testWebhook(ep.id)}
									disabled={testingId === ep.id}
								>
									{testingId === ep.id ? 'Testing…' : 'Test'}
								</button>
								<button class="btn-sm" onclick={() => openEdit(ep)}>Edit</button>
								<button class="btn-sm btn-danger" onclick={() => deleteEndpoint(ep.id)}>Delete</button>
							</td>
						</tr>
						{#if testResult && testingId === null && selectedId === null && ep.id === endpoints[endpoints.length - 1]?.id}
						<!-- shown inline after test -->
						{/if}
					{/each}
				</tbody>
			</table></div>
		{/if}
	</div>
</section>

{#if testResult && testingId === null}
<div class="test-result card {testResult.success ? 'green-border' : 'red-border'}">
	<strong>Test result:</strong>
	{#if testResult.success}
		<span class="green">Success (HTTP {testResult.status_code})</span>
	{:else}
		<span class="red">Failed — {testResult.error ?? `HTTP ${testResult.status_code}`}</span>
	{/if}
	<button class="btn-sm" onclick={() => (testResult = null)}>Dismiss</button>
</div>
{/if}

{#if selectedId && deliveries.length > 0}
<section>
	<div class="section-header">
		<h2>Delivery Log</h2>
		<button class="btn-sm" onclick={() => { selectedId = null; deliveries = []; }}>Close</button>
	</div>
	<div class="card">
		<div class="table-wrap"><table>
			<thead>
				<tr><th>Event</th><th>Status</th><th>Attempts</th><th>Created</th><th>Delivered</th><th>Error</th></tr>
			</thead>
			<tbody>
				{#each deliveries as d}
					<tr>
						<td class="mono">{d.event_type}</td>
						<td><span class="badge {statusBadge(d.status)}">{d.status}</span></td>
						<td>{d.attempts}</td>
						<td class="muted">{fmt(d.created_at)}</td>
						<td class="muted">{fmt(d.delivered_at)}</td>
						<td class="muted mono small">{d.last_error ?? '—'}</td>
					</tr>
				{/each}
			</tbody>
		</table></div>
	</div>
</section>
{:else if selectedId && deliveries.length === 0}
<section>
	<div class="card"><p class="muted">No deliveries found for this endpoint.</p></div>
</section>
{/if}

{#if showModal}
<div class="modal-backdrop" role="dialog" aria-modal="true">
	<div class="modal">
		<h2>{editId ? 'Edit Webhook' : 'Add Webhook'}</h2>
		<label>
			URL
			<input type="url" bind:value={formUrl} placeholder="https://example.com/webhook" />
		</label>
		<label>
			Description
			<input type="text" bind:value={formDesc} placeholder="Optional description" />
		</label>
		<label>
			Event Filter <span class="muted">(comma-separated; empty = all)</span>
			<input type="text" bind:value={formEventFilter}
				placeholder="deposit_confirmed, withdrawal_broadcast" />
		</label>
		<label class="checkbox-row">
			<input type="checkbox" bind:checked={formEnabled} />
			Enabled
		</label>
		{#if formError}<p class="error">{formError}</p>{/if}
		<div class="modal-actions">
			<button class="btn-primary" onclick={saveWebhook}>Save</button>
			<button class="btn-sm" onclick={() => (showModal = false)}>Cancel</button>
		</div>
	</div>
</div>
{/if}

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }
	h2 { font-size: 1rem; color: var(--fg-muted, #8b949e); margin: 0; }

	.metrics-bar {
		display: flex;
		gap: 1rem;
		margin-bottom: 1.2rem;
		flex-wrap: wrap;
	}

	.metric {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 0.8rem 1.2rem;
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		min-width: 130px;
	}

	.metric .label { font-size: 0.75rem; color: var(--fg-muted, #8b949e); }
	.metric .val   { font-size: 1.2rem; font-family: monospace; font-weight: 600; }

	section { margin-bottom: 1rem; }

	.section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.5rem;
	}

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1rem;
	}

	.green-border { border-color: var(--green, #3fb950); }
	.red-border   { border-color: var(--red, #f85149); }

	.test-result {
		margin-bottom: 1rem;
		display: flex;
		align-items: center;
		gap: 1rem;
		font-size: 0.9rem;
	}

	.table-wrap { overflow-x: auto; }
	table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.45rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); vertical-align: middle; }

	tr.selected td { background: rgba(48, 54, 61, 0.6); }

	.ep-url  { font-family: monospace; font-size: 0.85rem; word-break: break-all; }
	.ep-desc { font-size: 0.78rem; margin-top: 0.15rem; }

	.badge { border-radius: 4px; padding: 0.15rem 0.45rem; font-size: 0.72rem; font-weight: 600; }
	.green  { color: var(--green,  #3fb950); }
	.yellow { color: var(--yellow, #d29922); }
	.red    { color: var(--red,    #f85149); }

	.badge.green  { background: rgba(63,185,80,.15); }
	.badge.yellow { background: rgba(210,153,34,.15); }
	.badge.red    { background: rgba(248,81,73,.15); }

	.actions { display: flex; gap: 0.35rem; white-space: nowrap; }

	.btn-primary {
		background: var(--accent, #1f6feb);
		color: #fff;
		border: none;
		border-radius: 6px;
		padding: 0.4rem 0.9rem;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.btn-primary:hover { filter: brightness(1.1); }

	.btn-sm {
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		padding: 0.25rem 0.6rem;
		font-size: 0.78rem;
		cursor: pointer;
		color: inherit;
	}
	.btn-sm:hover { border-color: var(--accent, #1f6feb); }
	.btn-sm:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-danger { border-color: var(--red, #f85149); color: var(--red, #f85149); }
	.btn-danger:hover { background: rgba(248,81,73,.1); }

	.modal-backdrop {
		position: fixed; inset: 0;
		background: rgba(0,0,0,.6);
		display: flex; align-items: center; justify-content: center;
		z-index: 100;
	}

	.modal {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 10px;
		padding: 1.5rem;
		width: 480px;
		max-width: 95vw;
		display: flex;
		flex-direction: column;
		gap: 0.8rem;
	}

	.modal h2 { font-size: 1.1rem; color: inherit; }

	.modal label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.85rem;
		color: var(--fg-muted, #8b949e);
	}

	.modal input[type="url"],
	.modal input[type="text"] {
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		padding: 0.45rem 0.75rem;
		color: inherit;
		font-size: 0.9rem;
	}

	.checkbox-row {
		flex-direction: row !important;
		align-items: center;
		gap: 0.5rem !important;
		color: inherit !important;
	}

	.modal-actions { display: flex; gap: 0.5rem; justify-content: flex-end; margin-top: 0.5rem; }

	.error { color: var(--red, #f85149); font-size: 0.82rem; margin: 0; }

	.muted { color: var(--fg-muted, #8b949e); }
	.mono  { font-family: monospace; font-size: 0.84rem; }
	.small { font-size: 0.78rem; }
</style>
