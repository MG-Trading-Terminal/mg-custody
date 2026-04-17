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

	// ---------------------------------------------------------------------------
	// Retry queue types
	// ---------------------------------------------------------------------------

	interface RetryQueueEntry {
		endpoint_id: string;
		endpoint_url: string;
		pending_count: number;
		failed_count: number;
		last_error: string | null;
		next_retry_at: string | null;
		deliveries: Delivery[];
	}

	let endpoints    = $state<WebhookEndpoint[]>([]);
	let metrics      = $state<Metrics | null>(null);
	let deliveries   = $state<Delivery[]>([]);
	let selectedId   = $state<string | null>(null);
	let loading      = $state(true);
	let showModal    = $state(false);
	let testResult   = $state<{ success: boolean; status_code?: number; error?: string } | null>(null);
	let testingId    = $state<string | null>(null);

	// Retry queue state
	let retryQueue         = $state<RetryQueueEntry[]>([]);
	let retryQueueLoading  = $state(false);
	let retryingId         = $state<string | null>(null);
	let retryResult        = $state<Record<string, { count: number; error?: string }>>({});

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

	async function loadRetryQueue() {
		retryQueueLoading = true;
		const activeEndpoints = endpoints.filter(ep => ep.enabled);
		const results = await Promise.allSettled(
			activeEndpoints.map(async ep => {
				const res = await api.getJson<{ deliveries: Delivery[] }>(`/v1/admin/webhooks/${ep.id}/deliveries`);
				const all = res.deliveries ?? [];
				const failed  = all.filter(d => d.status === 'failed');
				const pending = all.filter(d => d.status === 'pending');
				const lastFailed = failed[0] ?? null;
				// next_retry_at isn't exposed in the delivery type; use created_at as fallback
				return {
					endpoint_id:   ep.id,
					endpoint_url:  ep.url,
					pending_count: pending.length,
					failed_count:  failed.length,
					last_error:    lastFailed?.last_error ?? null,
					next_retry_at: null,
					deliveries:    all.slice(0, 5),
				} satisfies RetryQueueEntry;
			})
		);
		retryQueue = results
			.map(r => r.status === 'fulfilled' ? r.value : null)
			.filter((r): r is RetryQueueEntry => r !== null)
			.filter(r => r.failed_count > 0 || r.pending_count > 0);
		retryQueueLoading = false;
	}

	async function retryNow(endpointId: string) {
		retryingId = endpointId;
		try {
			const res = await api.post<{ retried_count: number }>(`/v1/admin/webhooks/${endpointId}/retry`, {});
			retryResult = { ...retryResult, [endpointId]: { count: res.retried_count } };
			// Refresh queue after retry
			await loadRetryQueue();
		} catch (e: unknown) {
			retryResult = { ...retryResult, [endpointId]: { count: 0, error: String(e) } };
		} finally {
			retryingId = null;
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

	async function loadAll() {
		await load();
		await loadRetryQueue();
	}

	onMount(loadAll);
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

<!-- Retry Queue section -->
<section class="retry-section">
	<div class="section-header">
		<h2>Retry Queue</h2>
		<button class="btn-sm" onclick={loadRetryQueue} disabled={retryQueueLoading}>
			{retryQueueLoading ? 'Refreshing…' : 'Refresh'}
		</button>
	</div>

	{#if retryQueueLoading}
		<div class="card"><p class="muted">Loading retry queue…</p></div>
	{:else if retryQueue.length === 0}
		<div class="card"><p class="muted">No pending or failed deliveries. All webhooks are healthy.</p></div>
	{:else}
		{#each retryQueue as entry}
			<div class="card retry-card">
				<div class="retry-header">
					<div class="retry-url mono">{entry.endpoint_url}</div>
					<div class="retry-badges">
						{#if entry.pending_count > 0}
							<span class="badge yellow">{entry.pending_count} pending</span>
						{/if}
						{#if entry.failed_count > 0}
							<span class="badge red">{entry.failed_count} failed</span>
						{/if}
					</div>
					<button
						class="btn-sm btn-retry"
						onclick={() => retryNow(entry.endpoint_id)}
						disabled={retryingId === entry.endpoint_id}
					>
						{retryingId === entry.endpoint_id ? 'Retrying…' : 'Retry Now'}
					</button>
				</div>

				{#if entry.last_error}
					<div class="retry-last-error">
						<span class="muted small">Last error:</span>
						<span class="mono small red">{entry.last_error}</span>
					</div>
				{/if}

				{#if retryResult[entry.endpoint_id]}
					{#if retryResult[entry.endpoint_id].error}
						<p class="retry-feedback error">Retry failed: {retryResult[entry.endpoint_id].error}</p>
					{:else}
						<p class="retry-feedback green">{retryResult[entry.endpoint_id].count} deliveries queued for retry.</p>
					{/if}
				{/if}

				<!-- Last 5 delivery attempts timeline -->
				{#if entry.deliveries.length > 0}
					<div class="delivery-timeline">
						<div class="timeline-label muted small">Last {Math.min(entry.deliveries.length, 5)} attempts</div>
						<div class="timeline-entries">
							{#each entry.deliveries.slice(0, 5) as d}
								<div class="timeline-entry">
									<span class="badge {statusBadge(d.status)} badge-sm">{d.status}</span>
									<span class="mono small">{d.event_type}</span>
									<span class="muted small">×{d.attempts} attempts</span>
									<span class="muted small">{fmt(d.created_at)}</span>
									{#if d.last_error}
										<span class="mono small red ellipsis" title={d.last_error}>{d.last_error.slice(0, 40)}{d.last_error.length > 40 ? '…' : ''}</span>
									{/if}
								</div>
							{/each}
						</div>
					</div>
				{/if}
			</div>
		{/each}
	{/if}
</section>

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
	h2 { font-size: 1rem; color: var(--fg-muted); margin: 0; }

	.metrics-bar {
		display: flex;
		gap: 1rem;
		margin-bottom: 1.2rem;
		flex-wrap: wrap;
	}

	.metric {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		padding: 0.8rem 1.2rem;
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
		min-width: 130px;
	}

	.metric .label { font-size: 0.75rem; color: var(--fg-muted); }
	.metric .val   { font-size: 1.2rem; font-family: var(--font-mono); font-variant-numeric: tabular-nums; font-weight: 600; }

	section { margin-bottom: 1rem; }

	.section-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.5rem;
	}

	.card {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		padding: 1rem;
	}

	.green-border { border-color: var(--green); }
	.red-border   { border-color: var(--red); }

	.test-result {
		margin-bottom: 1rem;
		display: flex;
		align-items: center;
		gap: 1rem;
		font-size: 0.9rem;
	}

	.table-wrap { overflow-x: auto; }
	table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted); border-bottom: 1px solid var(--border); }
	td { padding: 0.45rem 0.75rem; border-bottom: 1px solid var(--border); vertical-align: middle; }

	tr.selected td { background: var(--accent-soft); }

	.ep-url  { font-family: var(--font-mono); font-variant-numeric: tabular-nums; font-size: 0.85rem; word-break: break-all; }
	.ep-desc { font-size: 0.78rem; margin-top: 0.15rem; }

	.badge { border-radius: var(--r-sm); padding: 0.15rem 0.45rem; font-size: 0.72rem; font-weight: 600; }
	.green  { color: var(--green,  #3fb950); }
	.yellow { color: var(--yellow); }
	.red    { color: var(--red,    #f85149); }

	.badge.green  { background: rgba(63,185,80,.15); }
	.badge.yellow { background: rgba(210,153,34,.15); }
	.badge.red    { background: rgba(248,81,73,.15); }

	.actions { display: flex; gap: 0.35rem; white-space: nowrap; }

	.btn-primary {
		background: var(--accent);
		color: #0A0B0D;
		border: none;
		border-radius: var(--r-sm);
		padding: 0.4rem 0.9rem;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.btn-primary:hover { filter: brightness(1.1); }

	.btn-sm {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		padding: 0.25rem 0.6rem;
		font-size: 0.78rem;
		cursor: pointer;
		color: inherit;
	}
	.btn-sm:hover { border-color: var(--accent); }
	.btn-sm:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-danger { border-color: var(--red); color: var(--red); }
	.btn-danger:hover { background: rgba(248,81,73,.1); }

	.modal-backdrop {
		position: fixed; inset: 0;
		background: rgba(0,0,0,.6);
		display: flex; align-items: center; justify-content: center;
		z-index: 100;
	}

	.modal {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
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
		color: var(--fg-muted);
	}

	.modal input[type="url"],
	.modal input[type="text"] {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
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

	.error { color: var(--red); font-size: 0.82rem; margin: 0; }

	.muted { color: var(--fg-muted); }
	.mono  { font-family: var(--font-mono); font-variant-numeric: tabular-nums; font-size: 0.84rem; }
	.small { font-size: 0.78rem; }

	/* ---------------------------------------------------------------------------
	   Retry queue styles
	---------------------------------------------------------------------------- */
	.retry-section { margin-top: 1.5rem; margin-bottom: 1rem; }

	.retry-card {
		margin-bottom: 0.75rem;
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	.retry-header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
	}

	.retry-url {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 0.84rem;
	}

	.retry-badges {
		display: flex;
		gap: 0.3rem;
	}

	.btn-retry {
		white-space: nowrap;
		border-color: var(--accent);
		color: var(--accent);
	}
	.btn-retry:hover { background: rgba(88,166,255,.1); }

	.retry-last-error {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.retry-feedback { margin: 0; font-size: 0.82rem; }

	/* Delivery timeline */
	.delivery-timeline {
		border-top: 1px solid var(--border);
		padding-top: 0.5rem;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	.timeline-label { margin-bottom: 0.15rem; }

	.timeline-entries {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
	}

	.timeline-entry {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		flex-wrap: wrap;
		padding: 0.2rem 0.4rem;
		border-radius: var(--r-sm);
		background: var(--surface-2);
	}

	.badge-sm { font-size: 0.66rem; padding: 0.08rem 0.35rem; }

	.ellipsis {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		max-width: 300px;
	}
</style>
