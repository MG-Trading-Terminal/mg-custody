<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { chainIcon, tokenIcon } from '$lib/icons/tokens';

	interface FeeEstimate {
		amount: string;
		network_fee: string;
		total: string;
		asset: string;
	}

	interface WithdrawalRecord {
		id: string;
		asset: string;
		chain: string;
		amount: string;
		destination: string;
		status: string;
		tx_hash?: string;
		created_at: string;
	}

	const CHAINS = ['ethereum', 'bitcoin', 'tron', 'solana'];
	const ASSETS_BY_CHAIN: Record<string, string[]> = {
		ethereum: ['ETH', 'USDT', 'USDC'],
		bitcoin:  ['BTC'],
		tron:     ['TRX', 'USDT'],
		solana:   ['SOL', 'USDT', 'USDC'],
	};

	let chain       = $state('ethereum');
	let asset       = $state('ETH');
	let amount      = $state('');
	let destination = $state('');

	let estimate    = $state<FeeEstimate | null>(null);
	let estLoading  = $state(false);
	let estError    = $state('');

	let submitting  = $state(false);
	let submitError = $state('');
	let submitOk    = $state('');

	let history     = $state<WithdrawalRecord[]>([]);
	let histLoading = $state(true);
	let histError   = $state('');

	$effect(() => {
		// Reset asset when chain changes
		const available = ASSETS_BY_CHAIN[chain] ?? [];
		if (!available.includes(asset)) {
			asset = available[0] ?? '';
		}
		estimate   = null;
		estError   = '';
	});

	async function estimateFee() {
		if (!amount || !destination) { estError = 'Enter amount and destination first'; return; }
		estLoading = true;
		estError   = '';
		estimate   = null;
		try {
			estimate = await api.post<FeeEstimate>('/v1/user/withdrawals/estimate', {
				chain, asset, amount, destination,
			});
		} catch (e) {
			estError = e instanceof Error ? e.message : 'Fee estimation failed';
		} finally {
			estLoading = false;
		}
	}

	async function submit() {
		if (!amount || !destination) { submitError = 'All fields are required'; return; }
		submitting  = true;
		submitError = '';
		submitOk    = '';
		try {
			const res = await api.post<{ custody_ref: string }>('/v1/withdrawals', {
				chain, asset, amount, address: destination,
			});
			submitOk    = `Withdrawal submitted. Reference: ${res.custody_ref}`;
			amount      = '';
			destination = '';
			estimate    = null;
			fetchHistory();
		} catch (e) {
			submitError = e instanceof Error ? e.message : 'Submission failed';
		} finally {
			submitting = false;
		}
	}

	async function fetchHistory() {
		histLoading = true;
		histError   = '';
		try {
			const data = await api.getJson<{ withdrawals: WithdrawalRecord[] }>('/v1/user/withdrawal-history');
			history = data.withdrawals ?? [];
		} catch (e) {
			histError = e instanceof Error ? e.message : 'Failed to load withdrawal history';
		} finally {
			histLoading = false;
		}
	}

	onMount(fetchHistory);

	const STATUS_COLOR: Record<string, string> = {
		confirmed: 'green',
		broadcast: 'yellow',
		pending:   'yellow',
		failed:    'red',
	};
	function statusClass(s: string) { return STATUS_COLOR[s] ?? 'muted'; }
	function trunc(s: string, n = 14) { return s.length > n ? s.slice(0, 8) + '…' + s.slice(-6) : s; }
	function fmt(iso: string) { return new Date(iso).toLocaleString(); }
</script>

<h1>Withdraw</h1>

<div class="card">
	<h3>New Withdrawal</h3>

	<div class="form-grid">
		<label class="field">
			Chain
			<select bind:value={chain}>
				{#each CHAINS as c}<option value={c}>{c.charAt(0).toUpperCase() + c.slice(1)}</option>{/each}
			</select>
		</label>

		<label class="field">
			Asset
			<select bind:value={asset}>
				{#each (ASSETS_BY_CHAIN[chain] ?? []) as a}<option value={a}>{a}</option>{/each}
			</select>
		</label>

		<label class="field span2">
			Amount
			<input type="text" bind:value={amount} placeholder="0.00" />
		</label>

		<label class="field span2">
			Destination Address
			<input type="text" bind:value={destination} placeholder="Paste destination address" class="mono" />
		</label>
	</div>

	<div class="form-actions">
		<button class="btn-secondary" onclick={estimateFee} disabled={estLoading}>
			{estLoading ? 'Estimating…' : 'Estimate Fee'}
		</button>
	</div>

	{#if estError}
		<p class="err">{estError}</p>
	{/if}

	{#if estimate}
		<div class="fee-breakdown">
			<div class="fee-row">
				<span>Amount</span>
				<span class="mono">{estimate.amount} {estimate.asset}</span>
			</div>
			<div class="fee-row">
				<span>Network Fee</span>
				<span class="mono fee-value">{estimate.network_fee} {estimate.asset}</span>
			</div>
			<div class="fee-row total-row">
				<span>Total Deducted</span>
				<span class="mono">{estimate.total} {estimate.asset}</span>
			</div>
		</div>
	{/if}

	{#if submitOk}
		<p class="ok">{submitOk}</p>
	{/if}
	{#if submitError}
		<p class="err">{submitError}</p>
	{/if}

	<button class="btn-primary" onclick={submit} disabled={submitting}>
		{submitting ? 'Submitting…' : 'Submit Withdrawal'}
	</button>
</div>

<div class="card">
	<h3>Recent Withdrawals</h3>
	{#if histLoading}
		<p class="muted">Loading…</p>
	{:else if histError}
		<p class="err">{histError}</p>
	{:else if history.length === 0}
		<p class="muted">No withdrawals yet.</p>
	{:else}
		<div class="table-wrap"><table>
			<thead>
				<tr><th>Asset</th><th>Chain</th><th>Amount</th><th>Destination</th><th>Tx Hash</th><th>Status</th><th>Date</th></tr>
			</thead>
			<tbody>
				{#each history as w}
					<tr>
						<td>{w.asset}</td>
						<td class="muted">{w.chain}</td>
						<td class="mono">{w.amount}</td>
						<td class="mono muted">{trunc(w.destination)}</td>
						<td class="mono muted">{w.tx_hash ? trunc(w.tx_hash) : '—'}</td>
						<td><span class="badge {statusClass(w.status)}">{w.status}</span></td>
						<td class="muted">{fmt(w.created_at)}</td>
					</tr>
				{/each}
			</tbody>
		</table></div>
	{/if}
</div>

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }
	h3 { margin: 0 0 0.9rem; font-size: 0.9rem; color: var(--fg-muted, #8b949e); text-transform: uppercase; letter-spacing: 0.05em; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
		margin-bottom: 1rem;
	}

	.form-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 0.75rem;
		margin-bottom: 1rem;
	}

	.field {
		display: flex; flex-direction: column; gap: 0.25rem;
		font-size: 0.8rem; color: var(--fg-muted, #8b949e);
	}
	.span2 { grid-column: span 2; }

	select, input[type="text"] {
		background: var(--bg, #0f1117);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		color: var(--fg, #e1e4e8);
		padding: 0.4rem 0.6rem;
		font-size: 0.9rem;
	}

	.form-actions { display: flex; gap: 0.75rem; margin-bottom: 0.75rem; }

	.fee-breakdown {
		background: var(--bg, #0f1117);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		padding: 0.75rem;
		margin-bottom: 0.75rem;
		font-size: 0.88rem;
	}
	.fee-row { display: flex; justify-content: space-between; padding: 0.3rem 0; }
	.fee-value { color: var(--yellow, #d29922); }
	.total-row {
		border-top: 1px solid var(--border, #30363d);
		margin-top: 0.25rem; padding-top: 0.5rem;
		font-weight: 600;
	}

	.btn-primary, .btn-secondary {
		border-radius: 6px; padding: 0.5rem 1.1rem;
		font-size: 0.9rem; font-weight: 600; cursor: pointer;
	}
	.btn-primary  { background: var(--accent, #58a6ff); color: #0f1117; border: none; }
	.btn-secondary { background: none; color: var(--accent, #58a6ff); border: 1px solid var(--accent, #58a6ff); }
	.btn-primary:disabled, .btn-secondary:disabled { opacity: 0.5; cursor: not-allowed; }

	.table-wrap { overflow-x: auto; -webkit-overflow-scrolling: touch; }
	table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); }
	tr:last-child td { border-bottom: none; }

	.badge { border-radius: 4px; padding: 0.15rem 0.5rem; font-size: 0.75rem; text-transform: capitalize; font-weight: 600; }
	.green  { background: rgba(63,185,80,.12);  color: var(--green, #3fb950); }
	.yellow { background: rgba(210,153,34,.12); color: var(--yellow, #d29922); }
	.red    { background: rgba(248,81,73,.12);  color: var(--red, #f85149); }
	.muted  { color: var(--fg-muted, #8b949e); }
	.mono   { font-family: monospace; }
	.err    { color: var(--red, #f85149); }
	.ok     { color: var(--green, #3fb950); font-size: 0.88rem; }

	@media (max-width: 768px) {
		.form-grid { grid-template-columns: 1fr; }
		.span2 { grid-column: span 1; }
	}
</style>
