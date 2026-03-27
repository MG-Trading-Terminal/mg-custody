<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface Vault { id: string; name: string; }

	interface Approval {
		id: string;
		withdrawal_id: string;
		asset: string;
		chain: string;
		amount: string;
		destination: string;
		requester: string;
		tier: number;
		approvals_given: number;
		approvals_required: number;
		expires_at: string;
		created_at: string;
	}

	let vaults      = $state<Vault[]>([]);
	let vaultId     = $state('');
	let approvals   = $state<Approval[]>([]);
	let loading     = $state(false);
	let error       = $state('');
	let acting      = $state<Record<string, boolean>>({});

	async function loadVaults() {
		try {
			const d = await api.getJson<{ vaults: Vault[] }>('/v1/vaults');
			vaults = d.vaults ?? [];
			if (vaults.length > 0 && !vaultId) { vaultId = vaults[0].id; await loadApprovals(); }
		} catch { /* ignore */ }
	}

	async function loadApprovals() {
		if (!vaultId) return;
		loading = true; error = '';
		try {
			const d = await api.getJson<{ approvals: Approval[] }>(`/v1/vaults/${vaultId}/approvals`);
			approvals = d.approvals ?? [];
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load approvals';
		} finally { loading = false; }
	}

	async function act(id: string, action: 'approve' | 'reject') {
		acting = { ...acting, [id]: true };
		try {
			await api.post(`/v1/approvals/${id}/${action}`);
			await loadApprovals();
		} catch (e) {
			error = e instanceof Error ? e.message : `Failed to ${action}`;
		} finally {
			const next = { ...acting }; delete next[id]; acting = next;
		}
	}

	onMount(loadVaults);

	function trunc(s: string) { return s.length > 18 ? s.slice(0, 9) + '…' + s.slice(-6) : s; }
	function timeLeft(iso: string): string {
		const diff = new Date(iso).getTime() - Date.now();
		if (diff <= 0) return 'Expired';
		const h = Math.floor(diff / 3_600_000);
		const m = Math.floor((diff % 3_600_000) / 60_000);
		return h > 0 ? `${h}h ${m}m` : `${m}m`;
	}
</script>

<h1>Pending Approvals</h1>

<div class="toolbar-top">
	{#if vaults.length > 0}
		<label>Vault
			<select bind:value={vaultId} onchange={loadApprovals}>
				{#each vaults as v}<option value={v.id}>{v.name}</option>{/each}
			</select>
		</label>
	{:else}
		<p class="muted">No vaults available.</p>
	{/if}
</div>

<div class="card">
	{#if loading}
		<p class="muted">Loading…</p>
	{:else if error}
		<p class="err">{error}</p>
	{:else if approvals.length === 0}
		<div class="empty">
			<div class="icon">A</div>
			<p>No pending approvals.</p>
			<p class="muted">Tier 2+ withdrawals requiring human sign-off will appear here.</p>
		</div>
	{:else}
		<table>
			<thead>
				<tr><th>Withdrawal</th><th>Asset</th><th>Amount</th><th>Destination</th><th>Tier</th><th>Progress</th><th>Expires</th><th></th></tr>
			</thead>
			<tbody>
				{#each approvals as a}
					<tr>
						<td class="mono muted">{trunc(a.withdrawal_id)}</td>
						<td>{a.asset} <span class="muted">({a.chain})</span></td>
						<td class="mono">{a.amount}</td>
						<td class="mono muted" title={a.destination}>{trunc(a.destination)}</td>
						<td><span class="badge tier{a.tier}">T{a.tier}</span></td>
						<td>{a.approvals_given}/{a.approvals_required}</td>
						<td class={timeLeft(a.expires_at) === 'Expired' ? 'red' : 'muted'}>{timeLeft(a.expires_at)}</td>
						<td class="actions">
							<button class="btn-approve" onclick={() => act(a.id, 'approve')} disabled={!!acting[a.id]}>
								{acting[a.id] ? '…' : 'Approve'}
							</button>
							<button class="btn-reject" onclick={() => act(a.id, 'reject')} disabled={!!acting[a.id]}>
								{acting[a.id] ? '…' : 'Reject'}
							</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	{/if}
</div>

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
	}

	.toolbar-top {
		display: flex;
		align-items: flex-end;
		gap: 1rem;
		margin-bottom: 1rem;
	}

	label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.78rem;
		color: var(--fg-muted, #8b949e);
	}

	select {
		background: var(--bg, #0f1117);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		color: var(--fg, #e1e4e8);
		padding: 0.35rem 0.6rem;
		font-size: 0.85rem;
	}

	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		min-height: 160px;
		gap: 0.4rem;
		text-align: center;
	}

	.icon {
		width: 2.8rem; height: 2.8rem;
		border-radius: 50%;
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		display: flex; align-items: center; justify-content: center;
		font-size: 1.2rem; font-weight: bold;
		color: var(--accent, #58a6ff);
	}

	table { width: 100%; border-collapse: collapse; font-size: 0.85rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.45rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); vertical-align: middle; }

	.actions { display: flex; gap: 0.4rem; }

	.btn-approve, .btn-reject {
		border: none; border-radius: 5px;
		padding: 0.25rem 0.65rem;
		font-size: 0.78rem; font-weight: 600;
		cursor: pointer;
	}
	.btn-approve { background: rgba(63,185,80,.15); color: var(--green, #3fb950); border: 1px solid var(--green, #3fb950); }
	.btn-reject  { background: rgba(248,81,73,.12); color: var(--red, #f85149);   border: 1px solid var(--red, #f85149); }
	.btn-approve:disabled, .btn-reject:disabled { opacity: 0.5; cursor: not-allowed; }

	.badge { border-radius: 4px; padding: 0.15rem 0.45rem; font-size: 0.75rem; font-weight: 700; }
	.tier2 { background: rgba(210,153,34,.15); color: var(--yellow, #d29922); }
	.tier3 { background: rgba(248,81,73,.12);  color: var(--red, #f85149); }
	.tier4 { background: rgba(248,81,73,.2);   color: #ff6b6b; }

	.mono  { font-family: monospace; font-size: 0.82rem; }
	.muted { color: var(--fg-muted, #8b949e); }
	.red   { color: var(--red, #f85149); }
	.err   { color: var(--red, #f85149); }
	p { margin: 0.25rem 0; font-size: 0.9rem; }
</style>
