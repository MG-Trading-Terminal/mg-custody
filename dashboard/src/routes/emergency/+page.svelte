<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

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

	let status       = $state<FreezeStatus | null>(null);
	let loading      = $state(true);
	let freezeReason = $state('');
	let actionMsg    = $state('');
	let actionError  = $state('');

	// Override form
	let overrideId     = $state('');
	let overrideReason = $state('');
	let overrideResult = $state<OverrideResponse | null>(null);
	let overrideError  = $state('');

	async function loadStatus() {
		loading = true;
		try {
			status = await api.getJson<FreezeStatus>('/v1/admin/emergency/status');
		} catch {
			status = null;
		}
		loading = false;
	}

	async function freeze() {
		if (!freezeReason.trim()) {
			actionError = 'Please enter a reason.';
			return;
		}
		if (!confirm(`Freeze ALL operations? Reason: ${freezeReason}`)) return;
		actionError = ''; actionMsg = '';
		try {
			const r = await api.post<FreezeResponse>('/v1/admin/emergency/freeze', {
				reason: freezeReason
			});
			actionMsg = `System frozen. Transitioned: ${r.transitioned}`;
			freezeReason = '';
			await loadStatus();
		} catch (e: unknown) {
			actionError = String(e);
		}
	}

	async function unfreeze() {
		if (!freezeReason.trim()) {
			actionError = 'Please enter a reason.';
			return;
		}
		if (!confirm(`Unfreeze operations? Reason: ${freezeReason}`)) return;
		actionError = ''; actionMsg = '';
		try {
			const r = await api.post<FreezeResponse>('/v1/admin/emergency/unfreeze', {
				reason: freezeReason
			});
			actionMsg = `System unfrozen. Transitioned: ${r.transitioned}`;
			freezeReason = '';
			await loadStatus();
		} catch (e: unknown) {
			actionError = String(e);
		}
	}

	async function forceComplete() {
		if (!overrideId.trim() || !overrideReason.trim()) {
			overrideError = 'Withdrawal ID and reason are required.';
			return;
		}
		if (!confirm(`Force-COMPLETE withdrawal ${overrideId}? This cannot be undone.`)) return;
		overrideError = ''; overrideResult = null;
		try {
			overrideResult = await api.post<OverrideResponse>(
				`/v1/admin/override/withdrawal/${overrideId}/force-complete`,
				{ reason: overrideReason }
			);
		} catch (e: unknown) {
			overrideError = String(e);
		}
	}

	async function forceCancel() {
		if (!overrideId.trim() || !overrideReason.trim()) {
			overrideError = 'Withdrawal ID and reason are required.';
			return;
		}
		if (!confirm(`Force-CANCEL withdrawal ${overrideId}? This cannot be undone.`)) return;
		overrideError = ''; overrideResult = null;
		try {
			overrideResult = await api.post<OverrideResponse>(
				`/v1/admin/override/withdrawal/${overrideId}/force-cancel`,
				{ reason: overrideReason }
			);
		} catch (e: unknown) {
			overrideError = String(e);
		}
	}

	function fmt(ts: string) {
		return new Date(ts).toLocaleString();
	}

	onMount(loadStatus);
</script>

<h1>Emergency Controls</h1>

<div class="warning-banner">
	<strong>OWNER-ONLY.</strong> All actions are logged to the immutable audit trail. Use only during verified incidents.
</div>

<!-- Freeze Status -->
<section>
	<h2>System State</h2>
	<div class="card">
		{#if loading}
			<p class="muted">Loading…</p>
		{:else if status}
			<div class="state-row">
				<div class="state-label">Current state</div>
				<span class="badge {status.frozen ? 'red' : 'green'} large">
					{status.frozen ? 'FROZEN' : 'OPERATIONAL'}
				</span>
			</div>
			<div class="state-row muted small">
				Last checked: {fmt(status.checked_at)}
			</div>
		{:else}
			<p class="muted">Status unavailable.</p>
		{/if}

		<div class="form-row">
			<input
				type="text"
				bind:value={freezeReason}
				placeholder="Reason (required)"
				class="reason-input"
			/>
			<button
				class="btn-danger"
				onclick={freeze}
				disabled={!status || status.frozen}
			>
				Freeze All Operations
			</button>
			<button
				class="btn-safe"
				onclick={unfreeze}
				disabled={!status || !status.frozen}
			>
				Unfreeze Operations
			</button>
			<button class="btn-sm" onclick={loadStatus}>Refresh</button>
		</div>

		{#if actionMsg}
			<p class="success">{actionMsg}</p>
		{/if}
		{#if actionError}
			<p class="error">{actionError}</p>
		{/if}
	</div>
</section>

<!-- Withdrawal Overrides -->
<section>
	<h2>Withdrawal Override</h2>
	<div class="card">
		<p class="muted">
			Force-complete or force-cancel a stuck withdrawal. Use only when a withdrawal
			is permanently stuck and the blockchain state has been independently confirmed.
		</p>

		<div class="form-col">
			<label>
				Withdrawal ID
				<input type="text" bind:value={overrideId} placeholder="UUID" class="id-input mono" />
			</label>
			<label>
				Justification (mandatory — written to audit log)
				<textarea
					bind:value={overrideReason}
					rows="2"
					placeholder="e.g. tx stuck for 72h, manually verified on-chain as unconfirmed"
				></textarea>
			</label>
		</div>

		<div class="action-row">
			<button class="btn-safe" onclick={forceComplete}>Force-Complete</button>
			<button class="btn-danger" onclick={forceCancel}>Force-Cancel</button>
		</div>

		{#if overrideError}
			<p class="error">{overrideError}</p>
		{/if}

		{#if overrideResult}
			<div class="override-result card">
				<div class="kv"><span>Withdrawal</span><span class="mono">{overrideResult.withdrawal_id}</span></div>
				<div class="kv"><span>Action</span><span class="badge yellow">{overrideResult.action}</span></div>
				<div class="kv"><span>Previous status</span><span class="muted">{overrideResult.previous_status}</span></div>
				<div class="kv"><span>New status</span><span class="badge green">{overrideResult.new_status}</span></div>
				<div class="kv"><span>Reason</span><span class="muted">{overrideResult.reason}</span></div>
				<div class="kv"><span>Timestamp</span><span class="muted">{fmt(overrideResult.timestamp)}</span></div>
			</div>
		{/if}
	</div>
</section>

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }
	h2 { font-size: 1rem; color: var(--fg-muted, #8b949e); margin: 0 0 0.5rem; }

	.warning-banner {
		background: rgba(248,81,73,.1);
		border: 1px solid var(--red, #f85149);
		border-radius: 8px;
		padding: 0.75rem 1rem;
		color: var(--red, #f85149);
		font-size: 0.88rem;
		margin-bottom: 1.2rem;
	}

	section { margin-bottom: 1.2rem; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.state-row {
		display: flex;
		align-items: center;
		gap: 1rem;
	}

	.state-label { font-size: 0.9rem; color: var(--fg-muted, #8b949e); }

	.badge { border-radius: 4px; padding: 0.15rem 0.45rem; font-size: 0.72rem; font-weight: 600; }
	.badge.large { font-size: 1rem; padding: 0.3rem 0.75rem; }
	.badge.green  { background: rgba(63,185,80,.15);  color: var(--green,  #3fb950); }
	.badge.red    { background: rgba(248,81,73,.15);  color: var(--red,    #f85149); }
	.badge.yellow { background: rgba(210,153,34,.15); color: var(--yellow, #d29922); }

	.form-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		flex-wrap: wrap;
	}

	.reason-input, .id-input {
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		padding: 0.4rem 0.75rem;
		color: inherit;
		font-size: 0.9rem;
	}

	.reason-input { flex: 1; min-width: 200px; }
	.id-input { width: 340px; max-width: 100%; }

	.form-col {
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}

	.form-col label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.85rem;
		color: var(--fg-muted, #8b949e);
	}

	textarea {
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		padding: 0.4rem 0.75rem;
		color: inherit;
		font-size: 0.88rem;
		resize: vertical;
	}

	.action-row {
		display: flex;
		gap: 0.75rem;
		flex-wrap: wrap;
	}

	.btn-danger {
		background: rgba(248,81,73,.15);
		border: 1px solid var(--red, #f85149);
		color: var(--red, #f85149);
		border-radius: 6px;
		padding: 0.4rem 0.9rem;
		font-size: 0.88rem;
		cursor: pointer;
	}
	.btn-danger:hover    { background: rgba(248,81,73,.25); }
	.btn-danger:disabled { opacity: 0.4; cursor: not-allowed; }

	.btn-safe {
		background: rgba(63,185,80,.1);
		border: 1px solid var(--green, #3fb950);
		color: var(--green, #3fb950);
		border-radius: 6px;
		padding: 0.4rem 0.9rem;
		font-size: 0.88rem;
		cursor: pointer;
	}
	.btn-safe:hover    { background: rgba(63,185,80,.2); }
	.btn-safe:disabled { opacity: 0.4; cursor: not-allowed; }

	.btn-sm {
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		padding: 0.35rem 0.7rem;
		font-size: 0.82rem;
		cursor: pointer;
		color: inherit;
	}

	.kv {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.3rem 0;
		border-bottom: 1px solid var(--border, #30363d);
		font-size: 0.88rem;
	}

	.override-result {
		border-color: var(--green, #3fb950) !important;
		margin-top: 0.5rem;
	}

	.success { color: var(--green, #3fb950); font-size: 0.88rem; margin: 0; }
	.error   { color: var(--red,   #f85149); font-size: 0.88rem; margin: 0; }
	.muted   { color: var(--fg-muted, #8b949e); }
	.mono    { font-family: monospace; font-size: 0.84rem; }
	.small   { font-size: 0.82rem; }
</style>
