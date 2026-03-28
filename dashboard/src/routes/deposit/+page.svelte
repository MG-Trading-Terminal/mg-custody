<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface DepositAddress {
		address: string;
		chain: string;
		memo?: string;
		qr_data: string;
	}

	interface DepositRecord {
		id: string;
		asset: string;
		amount: string;
		confirmations: number;
		required_confirmations: number;
		status: string;
		tx_hash: string;
		created_at: string;
	}

	const CHAINS = ['ethereum', 'bitcoin', 'tron', 'solana'];

	let selectedChain = $state('ethereum');
	let addrInfo      = $state<DepositAddress | null>(null);
	let addrLoading   = $state(false);
	let addrError     = $state('');
	let copied        = $state(false);

	let history     = $state<DepositRecord[]>([]);
	let histLoading = $state(true);
	let histError   = $state('');

	async function fetchAddress(chain: string) {
		addrLoading = true;
		addrError   = '';
		addrInfo    = null;
		try {
			addrInfo = await api.getJson<DepositAddress>(`/v1/user/deposit-address/${chain}`);
		} catch (e) {
			addrError = e instanceof Error ? e.message : 'Failed to fetch address';
		} finally {
			addrLoading = false;
		}
	}

	async function fetchHistory() {
		histLoading = true;
		histError   = '';
		try {
			const data = await api.getJson<{ deposits: DepositRecord[] }>('/v1/user/deposit-history');
			history = data.deposits ?? [];
		} catch (e) {
			histError = e instanceof Error ? e.message : 'Failed to load deposit history';
		} finally {
			histLoading = false;
		}
	}

	async function copyAddress() {
		if (!addrInfo) return;
		await navigator.clipboard.writeText(addrInfo.address);
		copied = true;
		setTimeout(() => { copied = false; }, 2000);
	}

	onMount(() => {
		fetchAddress(selectedChain);
		fetchHistory();
	});

	function confirmProgress(rec: DepositRecord): string {
		return `${rec.confirmations}/${rec.required_confirmations} confirmations`;
	}

	const STATUS_COLOR: Record<string, string> = {
		confirmed: 'green',
		pending:   'yellow',
		failed:    'red',
	};
	function statusClass(s: string) { return STATUS_COLOR[s] ?? 'muted'; }
	function trunc(s: string, n = 14) { return s.length > n ? s.slice(0, 8) + '…' + s.slice(-6) : s; }
	function fmt(iso: string) { return new Date(iso).toLocaleString(); }
</script>

<h1>Deposit</h1>

<div class="card">
	<h3>Select Chain</h3>
	<div class="chain-tabs">
		{#each CHAINS as chain}
			<button
				class="chain-tab"
				class:active={selectedChain === chain}
				onclick={() => { selectedChain = chain; fetchAddress(chain); }}
			>
				{chain}
			</button>
		{/each}
	</div>

	<div class="addr-section">
		{#if addrLoading}
			<p class="muted">Loading address…</p>
		{:else if addrError}
			<p class="err">{addrError}</p>
		{:else if addrInfo}
			<div class="qr-placeholder">
				<p class="qr-data">{addrInfo.qr_data}</p>
				<p class="qr-hint">QR code — scan with wallet app</p>
			</div>

			<p class="addr-label">Your {addrInfo.chain} deposit address</p>
			<div class="addr-row">
				<code class="addr-text">{addrInfo.address}</code>
				<button class="btn-copy" onclick={copyAddress}>
					{copied ? 'Copied!' : 'Copy'}
				</button>
			</div>

			{#if addrInfo.memo}
				<div class="memo-row">
					<span class="memo-label">Memo / Tag</span>
					<code class="memo-text">{addrInfo.memo}</code>
				</div>
			{/if}

			<p class="addr-notice">
				Only send {addrInfo.chain === 'ethereum' ? 'ETH or ERC-20 tokens' :
				           addrInfo.chain === 'bitcoin'  ? 'BTC' :
				           addrInfo.chain === 'tron'     ? 'TRX or TRC-20 tokens' :
				                                           'SOL or SPL tokens'} to this address.
				Sending the wrong asset will result in permanent loss.
			</p>
		{/if}
	</div>
</div>

<div class="card">
	<h3>Deposit History</h3>
	{#if histLoading}
		<p class="muted">Loading…</p>
	{:else if histError}
		<p class="err">{histError}</p>
	{:else if history.length === 0}
		<p class="muted">No deposits yet.</p>
	{:else}
		<div class="table-wrap"><table>
			<thead>
				<tr><th>Asset</th><th>Amount</th><th>Tx Hash</th><th>Confirmations</th><th>Status</th><th>Date</th></tr>
			</thead>
			<tbody>
				{#each history as dep}
					<tr>
						<td>{dep.asset}</td>
						<td class="mono">{dep.amount}</td>
						<td class="mono muted">{trunc(dep.tx_hash)}</td>
						<td class="muted">{confirmProgress(dep)}</td>
						<td><span class="badge {statusClass(dep.status)}">{dep.status}</span></td>
						<td class="muted">{fmt(dep.created_at)}</td>
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

	.chain-tabs { display: flex; gap: 0.5rem; margin-bottom: 1.2rem; flex-wrap: wrap; }

	.chain-tab {
		background: none; border: 1px solid var(--border, #30363d);
		border-radius: 6px; padding: 0.4rem 0.9rem;
		color: var(--fg-muted, #8b949e); font-size: 0.85rem;
		text-transform: capitalize; cursor: pointer; transition: all 0.15s;
	}
	.chain-tab.active {
		border-color: var(--accent, #58a6ff);
		color: var(--accent, #58a6ff);
		background: rgba(88,166,255,.08);
	}

	.addr-section { margin-top: 0.5rem; }

	.qr-placeholder {
		width: 160px; height: 160px;
		border: 2px dashed var(--border, #30363d);
		border-radius: 8px;
		display: flex; flex-direction: column;
		align-items: center; justify-content: center;
		margin: 0 auto 1.2rem;
		padding: 0.75rem; text-align: center;
	}
	.qr-data { font-family: monospace; font-size: 0.6rem; color: var(--fg-muted, #8b949e); word-break: break-all; margin: 0 0 0.4rem; }
	.qr-hint { font-size: 0.7rem; color: var(--fg-muted, #8b949e); margin: 0; }

	.addr-label { font-size: 0.8rem; color: var(--fg-muted, #8b949e); margin: 0 0 0.4rem; text-transform: capitalize; }

	.addr-row {
		display: flex; align-items: center; gap: 0.75rem;
		background: var(--bg, #0f1117);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px; padding: 0.6rem 0.8rem;
		margin-bottom: 0.75rem;
	}
	.addr-text { font-family: monospace; font-size: 0.85rem; flex: 1; word-break: break-all; }

	.btn-copy {
		background: var(--accent, #58a6ff); color: #0f1117;
		border: none; border-radius: 5px;
		padding: 0.3rem 0.75rem; font-size: 0.8rem; font-weight: 600;
		cursor: pointer; white-space: nowrap; flex-shrink: 0;
	}

	.memo-row {
		display: flex; align-items: center; gap: 0.75rem;
		background: rgba(210,153,34,.06);
		border: 1px solid rgba(210,153,34,.25);
		border-radius: 6px; padding: 0.5rem 0.8rem;
		margin-bottom: 0.75rem;
	}
	.memo-label { font-size: 0.78rem; color: var(--yellow, #d29922); font-weight: 600; white-space: nowrap; }
	.memo-text  { font-family: monospace; font-size: 0.85rem; }

	.addr-notice {
		font-size: 0.78rem; color: var(--fg-muted, #8b949e);
		background: rgba(248,81,73,.06);
		border: 1px solid rgba(248,81,73,.2);
		border-radius: 6px; padding: 0.6rem 0.8rem; margin: 0;
	}

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
</style>
