<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { icon } from '$lib/icons/ui';

	interface FreezeStatus {
		frozen: boolean;
		checked_at: string;
	}

	interface FreezeResponse {
		frozen: boolean;
		transitioned: boolean;
		reason: string | null;
		timestamp: string;
	}

	interface OverrideResponse {
		withdrawal_id: string;
		action: string;
		previous_status: string;
		new_status: string;
		reason: string;
		timestamp: string;
	}

	let status = $state<FreezeStatus | null>(null);
	let loading = $state(true);
	let freezeReason = $state('');
	let actionMsg = $state('');
	let actionError = $state('');
	let working = $state<'freeze' | 'unfreeze' | null>(null);

	let overrideId = $state('');
	let overrideReason = $state('');
	let overrideResult = $state<OverrideResponse | null>(null);
	let overrideError = $state('');
	let overrideWorking = $state<'complete' | 'cancel' | null>(null);

	async function loadStatus() {
		loading = true;
		try {
			status = await api.getJson<FreezeStatus>('/v1/admin/emergency/status');
		} catch { status = null; }
		loading = false;
	}

	async function freeze() {
		if (!freezeReason.trim()) { actionError = 'Please enter a reason.'; return; }
		if (!confirm(`Freeze ALL operations?\n\nReason: ${freezeReason}`)) return;
		actionError = ''; actionMsg = ''; working = 'freeze';
		try {
			const r = await api.post<FreezeResponse>('/v1/admin/emergency/freeze', { reason: freezeReason });
			actionMsg = r.transitioned ? 'System frozen successfully' : 'System was already frozen';
			freezeReason = '';
			await loadStatus();
		} catch (e: unknown) { actionError = String(e); }
		finally { working = null; }
	}

	async function unfreeze() {
		if (!freezeReason.trim()) { actionError = 'Please enter a reason.'; return; }
		if (!confirm(`Unfreeze operations?\n\nReason: ${freezeReason}`)) return;
		actionError = ''; actionMsg = ''; working = 'unfreeze';
		try {
			const r = await api.post<FreezeResponse>('/v1/admin/emergency/unfreeze', { reason: freezeReason });
			actionMsg = r.transitioned ? 'System unfrozen successfully' : 'System was already operational';
			freezeReason = '';
			await loadStatus();
		} catch (e: unknown) { actionError = String(e); }
		finally { working = null; }
	}

	async function forceComplete() {
		if (!overrideId.trim() || !overrideReason.trim()) {
			overrideError = 'Withdrawal ID and reason are required.';
			return;
		}
		if (!confirm(`Force-COMPLETE withdrawal ${overrideId}?\n\nThis cannot be undone.`)) return;
		overrideError = ''; overrideResult = null; overrideWorking = 'complete';
		try {
			overrideResult = await api.post<OverrideResponse>(
				`/v1/admin/override/withdrawal/${overrideId}/force-complete`,
				{ reason: overrideReason }
			);
		} catch (e: unknown) { overrideError = String(e); }
		finally { overrideWorking = null; }
	}

	async function forceCancel() {
		if (!overrideId.trim() || !overrideReason.trim()) {
			overrideError = 'Withdrawal ID and reason are required.';
			return;
		}
		if (!confirm(`Force-CANCEL withdrawal ${overrideId}?\n\nThis cannot be undone.`)) return;
		overrideError = ''; overrideResult = null; overrideWorking = 'cancel';
		try {
			overrideResult = await api.post<OverrideResponse>(
				`/v1/admin/override/withdrawal/${overrideId}/force-cancel`,
				{ reason: overrideReason }
			);
		} catch (e: unknown) { overrideError = String(e); }
		finally { overrideWorking = null; }
	}

	function fmt(ts: string): string {
		return new Date(ts).toLocaleString();
	}

	onMount(loadStatus);
</script>

<!-- ═══ Head ═══ -->
<div class="page-head">
	<div>
		<h1>Emergency</h1>
		<p class="page-sub">Owner-level kill-switches and manual withdrawal overrides. Every action is written to the immutable audit log.</p>
	</div>
</div>

<!-- ═══ Warning banner ═══ -->
<div class="warning-banner">
	<span class="warn-icon">{@html icon('alert-triangle', 18)}</span>
	<div>
		<span class="warn-title">Owner-only operations</span>
		<span class="warn-sub">These controls bypass normal approval flows. Use only during verified incidents — all changes are irreversible from this interface.</span>
	</div>
</div>

<!-- ═══ System state ═══ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>
				<span class="panel-icon">{@html icon('shield', 14)}</span>
				System state
			</h2>
			<p class="panel-sub">Global freeze halts all signing and withdrawal processing</p>
		</div>

		{#if loading}
			<span class="chip tone-unknown"><span class="chip-dot"></span>Loading</span>
		{:else if status}
			{#if status.frozen}
				<span class="state-badge frozen">
					<span class="dot"></span>
					<span>FROZEN</span>
				</span>
			{:else}
				<span class="state-badge ok">
					<span class="dot"></span>
					<span>OPERATIONAL</span>
				</span>
			{/if}
		{:else}
			<span class="chip tone-unknown"><span class="chip-dot"></span>Unknown</span>
		{/if}
	</header>

	<div class="panel-body">
		{#if status}
			<span class="muted tiny">Last checked: {fmt(status.checked_at)}</span>
		{/if}

		<div class="freeze-form">
			<div class="field">
				<label for="freeze-reason">Reason (required — written to audit log)</label>
				<input
					id="freeze-reason"
					type="text"
					bind:value={freezeReason}
					placeholder="e.g. Anomalous outflow detected on Ethereum — investigating"
				/>
			</div>

			<div class="freeze-actions">
				<button
					class="btn btn-danger"
					onclick={freeze}
					disabled={!status || status.frozen || working !== null}
				>
					{#if working === 'freeze'}
						<span class="spinner"></span>
						<span>Freezing…</span>
					{:else}
						<span>{@html icon('alert-triangle', 14)}</span>
						<span>Freeze all operations</span>
					{/if}
				</button>
				<button
					class="btn btn-success"
					onclick={unfreeze}
					disabled={!status || !status.frozen || working !== null}
				>
					{#if working === 'unfreeze'}
						<span class="spinner"></span>
						<span>Unfreezing…</span>
					{:else}
						<span>{@html icon('check', 14)}</span>
						<span>Unfreeze</span>
					{/if}
				</button>
				<button class="btn btn-ghost" onclick={loadStatus}>
					<span>{@html icon('refresh', 14)}</span>
					<span>Refresh</span>
				</button>
			</div>

			{#if actionMsg}
				<div class="alert success">
					<span>{@html icon('check', 14)}</span>
					<span>{actionMsg}</span>
				</div>
			{/if}
			{#if actionError}
				<div class="alert danger">
					<span>{@html icon('alert-triangle', 14)}</span>
					<span>{actionError}</span>
				</div>
			{/if}
		</div>
	</div>
</section>

<!-- ═══ Withdrawal override ═══ -->
<section class="panel">
	<header class="panel-head">
		<div>
			<h2>
				<span class="panel-icon">{@html icon('sliders', 14)}</span>
				Withdrawal override
			</h2>
			<p class="panel-sub">Force-complete or cancel a stuck withdrawal when on-chain state has been independently verified</p>
		</div>
	</header>

	<div class="panel-body">
		<div class="field">
			<label for="ovr-id">Withdrawal ID</label>
			<input id="ovr-id" type="text" bind:value={overrideId} placeholder="UUID" class="mono" />
		</div>

		<div class="field">
			<label for="ovr-reason">Justification (mandatory — written to audit log)</label>
			<textarea
				id="ovr-reason"
				bind:value={overrideReason}
				rows="2"
				placeholder="e.g. tx stuck for 72h, manually verified on-chain as unconfirmed, no mempool presence"
			></textarea>
		</div>

		<div class="freeze-actions">
			<button class="btn btn-success" onclick={forceComplete} disabled={overrideWorking !== null}>
				{#if overrideWorking === 'complete'}
					<span class="spinner"></span>
					<span>Working…</span>
				{:else}
					<span>{@html icon('check', 14)}</span>
					<span>Force-complete</span>
				{/if}
			</button>
			<button class="btn btn-danger" onclick={forceCancel} disabled={overrideWorking !== null}>
				{#if overrideWorking === 'cancel'}
					<span class="spinner"></span>
					<span>Working…</span>
				{:else}
					<span>{@html icon('x', 14)}</span>
					<span>Force-cancel</span>
				{/if}
			</button>
		</div>

		{#if overrideError}
			<div class="alert danger">
				<span>{@html icon('alert-triangle', 14)}</span>
				<span>{overrideError}</span>
			</div>
		{/if}

		{#if overrideResult}
			<div class="result-card">
				<header class="result-head">
					<span>{@html icon('check', 14)}</span>
					<span>Override applied</span>
				</header>
				<dl class="result-list">
					<div><dt>Withdrawal</dt><dd class="mono">{overrideResult.withdrawal_id}</dd></div>
					<div><dt>Action</dt><dd><span class="chip tone-warn">{overrideResult.action}</span></dd></div>
					<div><dt>Status transition</dt><dd><span class="mono muted">{overrideResult.previous_status}</span> <span>→</span> <span class="chip tone-ok">{overrideResult.new_status}</span></dd></div>
					<div><dt>Reason</dt><dd>{overrideResult.reason}</dd></div>
					<div><dt>Timestamp</dt><dd class="mono muted">{fmt(overrideResult.timestamp)}</dd></div>
				</dl>
			</div>
		{/if}
	</div>
</section>

<style>
	.page-head { margin-bottom: var(--sp-5); }
	h1 { margin: 0 0 4px; font-size: 1.5rem; font-weight: 650; letter-spacing: -0.022em; }
	.page-sub { margin: 0; color: var(--fg-muted); font-size: 0.88rem; max-width: 720px; }

	.warning-banner {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		padding: var(--sp-4) var(--sp-5);
		background: linear-gradient(135deg, var(--red-soft) 0%, rgba(248, 113, 113, 0.04) 100%);
		border: 1px solid rgba(248, 113, 113, 0.35);
		border-radius: var(--r-md);
		color: var(--red);
		margin-bottom: var(--sp-4);
	}
	.warn-icon { display: inline-flex; flex-shrink: 0; margin-top: 1px; }
	.warning-banner > div { display: flex; flex-direction: column; gap: 2px; }
	.warn-title { font-weight: 600; font-size: 0.92rem; }
	.warn-sub { font-size: 0.82rem; opacity: 0.88; }

	.panel {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		box-shadow: var(--shadow-1);
		margin-bottom: var(--sp-4);
		overflow: hidden;
	}
	.panel-head {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: var(--sp-3);
		padding: var(--sp-4) var(--sp-5);
		border-bottom: 1px solid var(--border);
	}
	.panel-head h2 {
		margin: 0 0 2px;
		font-size: 0.92rem;
		font-weight: 600;
		color: var(--fg-strong);
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.panel-icon { display: inline-flex; color: var(--accent); }
	.panel-sub { margin: 0; font-size: 0.78rem; color: var(--fg-muted); }

	.panel-body {
		padding: var(--sp-5);
		display: flex;
		flex-direction: column;
		gap: var(--sp-4);
	}

	/* State badge */
	.state-badge {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 5px 12px;
		font-size: 0.82rem;
		font-weight: 700;
		letter-spacing: 0.06em;
		border-radius: var(--r-full);
		border: 1px solid transparent;
	}
	.state-badge .dot { width: 7px; height: 7px; border-radius: 50%; background: currentColor; }
	.state-badge.ok {
		color: var(--green);
		background: var(--green-soft);
		border-color: rgba(52, 211, 153, 0.3);
	}
	.state-badge.ok .dot { animation: pulse-ok 2s infinite; }
	@keyframes pulse-ok {
		0%   { box-shadow: 0 0 0 0 rgba(52, 211, 153, 0.5); }
		70%  { box-shadow: 0 0 0 8px rgba(52, 211, 153, 0); }
		100% { box-shadow: 0 0 0 0 rgba(52, 211, 153, 0); }
	}
	.state-badge.frozen {
		color: var(--red);
		background: var(--red-soft);
		border-color: rgba(248, 113, 113, 0.4);
		animation: blink 1.2s infinite;
	}
	@keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0.65; } }

	.chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 0.72rem;
		font-weight: 500;
		padding: 2px 9px;
		border-radius: var(--r-full);
		border: 1px solid var(--border);
	}
	.chip.tone-ok { color: var(--green); background: var(--green-soft); border-color: rgba(52, 211, 153, 0.3); }
	.chip.tone-warn { color: var(--yellow); background: var(--yellow-soft); border-color: rgba(245, 185, 66, 0.3); }
	.chip.tone-unknown { color: var(--fg-muted); background: var(--surface-2); }
	.chip-dot { width: 5px; height: 5px; border-radius: 50%; background: currentColor; }

	/* Forms */
	.freeze-form { display: flex; flex-direction: column; gap: var(--sp-3); }
	.field { display: flex; flex-direction: column; gap: 6px; }
	.field label {
		font-size: 0.72rem;
		font-weight: 600;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}
	.field input, .field textarea {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
		color: var(--fg);
		font-size: 0.88rem;
		padding: 9px 12px;
		outline: none;
		font-family: inherit;
		resize: vertical;
	}
	.field input.mono { font-family: var(--font-mono); }
	.field input:focus, .field textarea:focus { border-color: var(--accent); box-shadow: var(--shadow-focus); }

	.freeze-actions {
		display: flex;
		gap: var(--sp-2);
		flex-wrap: wrap;
	}

	.btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 8px 14px;
		font-size: 0.85rem;
		font-weight: 600;
		border-radius: var(--r-sm);
		border: 1px solid var(--border);
		background: var(--surface-2);
		color: var(--fg);
		cursor: pointer;
		transition: background 0.15s, border-color 0.15s;
	}
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-ghost { background: transparent; border-color: transparent; color: var(--fg-muted); font-weight: 500; }
	.btn-ghost:hover:not(:disabled) { background: var(--hover-bg); color: var(--fg); border-color: var(--border); }
	.btn-danger {
		background: var(--red);
		color: #fff;
		border-color: var(--red);
	}
	.btn-danger:hover:not(:disabled) { background: #e66565; border-color: #e66565; }
	.btn-success {
		background: var(--green);
		color: #0A0B0D;
		border-color: var(--green);
	}
	.btn-success:hover:not(:disabled) { background: #2fbb86; border-color: #2fbb86; }

	.spinner {
		width: 13px; height: 13px;
		border: 2px solid currentColor;
		border-top-color: transparent;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
		opacity: 0.6;
	}
	@keyframes spin { to { transform: rotate(360deg); } }

	.alert {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 9px 12px;
		border-radius: var(--r-sm);
		font-size: 0.82rem;
	}
	.alert.success { background: var(--green-soft); color: var(--green); border: 1px solid rgba(52, 211, 153, 0.25); }
	.alert.danger { background: var(--red-soft); color: var(--red); border: 1px solid rgba(248, 113, 113, 0.25); }

	/* Result card */
	.result-card {
		background: var(--green-soft);
		border: 1px solid rgba(52, 211, 153, 0.3);
		border-radius: var(--r-md);
		padding: var(--sp-3) var(--sp-4);
	}
	.result-head {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--green);
		font-weight: 600;
		font-size: 0.88rem;
		margin-bottom: var(--sp-3);
		padding-bottom: var(--sp-3);
		border-bottom: 1px solid rgba(52, 211, 153, 0.2);
	}
	.result-list { margin: 0; padding: 0; display: flex; flex-direction: column; gap: 6px; }
	.result-list > div { display: flex; justify-content: space-between; align-items: center; gap: var(--sp-3); font-size: 0.84rem; }
	.result-list dt { color: var(--fg-muted); font-weight: 500; flex-shrink: 0; }
	.result-list dd { margin: 0; color: var(--fg); text-align: right; display: inline-flex; align-items: center; gap: 8px; }

	.muted { color: var(--fg-muted); }
	.tiny { font-size: 0.72rem; }
</style>
