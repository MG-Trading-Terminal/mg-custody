<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';

	interface ChainHealth {
		chain: string;
		rpc: string;
		confirmations: number | string;
		status: 'ok' | 'degraded' | 'down';
		block_height: number | null;
		latency_ms: number | null;
	}

	interface CircuitBreaker {
		label: string;
		limit: string;
		current: string;
		tripped: boolean;
	}

	interface FidoInfo {
		user_id: string;
		credential_count: number;
	}

	let chains    = $state<ChainHealth[]>([]);
	let circuit   = $state<CircuitBreaker[]>([]);
	let fido      = $state<FidoInfo[]>([]);
	let loading   = $state(true);

	const STATIC_CIRCUIT = [
		{ label: 'Per-transaction cap',  limit: '$500,000' },
		{ label: 'Hourly outflow limit', limit: '$10,000,000' },
		{ label: 'Daily outflow limit',  limit: '$50,000,000' },
	];

	async function load() {
		loading = true;
		const [chainsRes, circuitRes, fidoRes] = await Promise.allSettled([
			api.getJson<{ chains: ChainHealth[] }>('/health/chains'),
			api.getJson<{ limits: CircuitBreaker[] }>('/v1/circuit-breaker/status'),
			api.getJson<{ credentials: FidoInfo[] }>('/v1/fido/credentials'),
		]);
		chains  = chainsRes.status  === 'fulfilled' ? (chainsRes.value.chains   ?? []) : [];
		circuit = circuitRes.status === 'fulfilled' ? (circuitRes.value.limits  ?? []) : [];
		fido    = fidoRes.status    === 'fulfilled' ? (fidoRes.value.credentials ?? []) : [];
		loading = false;
	}

	onMount(load);

	function statusClass(s: string) {
		if (s === 'ok')       return 'green';
		if (s === 'degraded') return 'yellow';
		return 'red';
	}
</script>

<h1>System Settings</h1>

<section>
	<h2>Chain Health</h2>
	<div class="card">
		{#if loading}
			<p class="muted">Loading…</p>
		{:else if chains.length === 0}
			<table>
				<thead><tr><th>Chain</th><th>RPC</th><th>Confirmations</th><th>Status</th></tr></thead>
				<tbody>
					{#each [
						{ chain: 'Bitcoin',  rpc: 'bitcoind @ :8332',    confirmations: 3,          status: 'unknown' },
						{ chain: 'Ethereum', rpc: 'execution node @ :8545', confirmations: 12,       status: 'unknown' },
						{ chain: 'Tron',     rpc: 'tron-grid @ :8090',   confirmations: 20,         status: 'unknown' },
						{ chain: 'Solana',   rpc: 'validator RPC @ :8899', confirmations: 'finalized', status: 'unknown' },
					] as c}
						<tr>
							<td>{c.chain}</td>
							<td class="mono muted">{c.rpc}</td>
							<td>{c.confirmations}</td>
							<td class="muted">—</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{:else}
			<table>
				<thead><tr><th>Chain</th><th>RPC</th><th>Conf.</th><th>Block</th><th>Latency</th><th>Status</th></tr></thead>
				<tbody>
					{#each chains as c}
						<tr>
							<td>{c.chain}</td>
							<td class="mono muted">{c.rpc}</td>
							<td>{c.confirmations}</td>
							<td class="mono muted">{c.block_height ?? '—'}</td>
							<td class="muted">{c.latency_ms != null ? `${c.latency_ms}ms` : '—'}</td>
							<td><span class="badge {statusClass(c.status)}">{c.status}</span></td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
		<p class="note muted">Override via <code>CUSTODY_CHAINS__*</code> environment variables.</p>
	</div>
</section>

<section>
	<h2>Circuit Breaker</h2>
	<div class="card">
		{#if circuit.length > 0}
			{#each circuit as row}
				<div class="kv">
					<span>{row.label}</span>
					<div class="kv-right">
						<span class="muted">{row.current} / </span><span class="val">{row.limit}</span>
						{#if row.tripped}<span class="badge red">TRIPPED</span>{/if}
					</div>
				</div>
			{/each}
		{:else}
			{#each STATIC_CIRCUIT as row}
				<div class="kv">
					<span>{row.label}</span>
					<span class="val">{row.limit}</span>
				</div>
			{/each}
		{/if}
		<p class="note muted">Trips fire a PagerDuty/webhook alert. State is persisted in PostgreSQL.</p>
	</div>
</section>

<section>
	<h2>FIDO2 Credentials</h2>
	<div class="card">
		{#if loading}
			<p class="muted">Loading…</p>
		{:else if fido.length === 0}
			<p class="muted">No FIDO2 credentials registered, or endpoint unavailable.</p>
		{:else}
			<table>
				<thead><tr><th>User</th><th>Credentials</th></tr></thead>
				<tbody>
					{#each fido as f}
						<tr>
							<td class="mono">{f.user_id}</td>
							<td>{f.credential_count}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		{/if}
	</div>
</section>

<section>
	<h2>Webhooks</h2>
	<div class="card empty">
		<p class="muted">Webhook endpoint configuration coming soon.</p>
		<p class="muted">Events: <code>deposit_confirmed</code>, <code>withdrawal_broadcast</code>, <code>withdrawal_confirmed</code>, <code>withdrawal_failed</code>.</p>
	</div>
</section>

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }
	h2 { font-size: 1rem; color: var(--fg-muted, #8b949e); margin: 1.2rem 0 0.5rem; }

	section { margin-bottom: 0.5rem; }

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1.2rem;
	}

	.empty { color: var(--fg-muted, #8b949e); }

	table { width: 100%; border-collapse: collapse; font-size: 0.9rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.5rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); }

	.badge { border-radius: 4px; padding: 0.15rem 0.45rem; font-size: 0.75rem; font-weight: 600; }
	.green  { background: rgba(63,185,80,.12);  color: var(--green, #3fb950); }
	.yellow { background: rgba(210,153,34,.12); color: var(--yellow, #d29922); }
	.red    { background: rgba(248,81,73,.12);  color: var(--red, #f85149); }

	.mono  { font-family: monospace; font-size: 0.85rem; }
	.muted { color: var(--fg-muted, #8b949e); }

	.kv {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 0.45rem 0;
		border-bottom: 1px solid var(--border, #30363d);
		font-size: 0.9rem;
	}

	.kv-right { display: flex; align-items: center; gap: 0.5rem; }
	.val { font-family: monospace; color: var(--yellow, #d29922); }

	.note { margin: 0.7rem 0 0; font-size: 0.8rem; }

	code {
		background: var(--hover-bg, #1c2128);
		border-radius: 4px;
		padding: 0.1rem 0.35rem;
		font-size: 0.78rem;
	}

	p { margin: 0.3rem 0; font-size: 0.9rem; }
</style>
