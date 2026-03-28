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

	interface DetailedChainHealth {
		status: string;
		rpc_url: string;
		latest_block: number | null;
		scanner_block: number | null;
		scanner_lag: number | null;
		last_scan_at: string | null;
		avg_rpc_latency_ms: number | null;
		error_rate_pct: number;
		deposits_today: number;
		withdrawals_today: number;
	}

	interface SystemHealth {
		uptime_seconds: number;
		db_pool_size: number;
		db_pool_active: number;
		memory_mb: number;
		pending_webhooks: number;
		circuit_breaker: string;
	}

	interface DetailedHealth {
		chains: Record<string, DetailedChainHealth>;
		system: SystemHealth;
		generated_at: string;
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

	let chains         = $state<ChainHealth[]>([]);
	let detailedHealth = $state<DetailedHealth | null>(null);
	let circuit        = $state<CircuitBreaker[]>([]);
	let fido           = $state<FidoInfo[]>([]);
	let loading        = $state(true);

	const STATIC_CIRCUIT = [
		{ label: 'Per-transaction cap',  limit: '$500,000' },
		{ label: 'Hourly outflow limit', limit: '$10,000,000' },
		{ label: 'Daily outflow limit',  limit: '$50,000,000' },
	];

	async function load() {
		loading = true;
		const [chainsRes, circuitRes, fidoRes, detailedRes] = await Promise.allSettled([
			api.getJson<{ chains: ChainHealth[] }>('/health/chains'),
			api.getJson<{ limits: CircuitBreaker[] }>('/v1/circuit-breaker/status'),
			api.getJson<{ credentials: FidoInfo[] }>('/v1/fido/credentials'),
			api.getJson<DetailedHealth>('/v1/health/detailed'),
		]);
		chains         = chainsRes.status  === 'fulfilled' ? (chainsRes.value.chains   ?? []) : [];
		circuit        = circuitRes.status === 'fulfilled' ? (circuitRes.value.limits  ?? []) : [];
		fido           = fidoRes.status    === 'fulfilled' ? (fidoRes.value.credentials ?? []) : [];
		detailedHealth = detailedRes.status === 'fulfilled' ? detailedRes.value : null;
		loading = false;
	}

	onMount(load);

	function statusClass(s: string) {
		if (s === 'ok' || s === 'healthy') return 'green';
		if (s === 'degraded' || s === 'lagging') return 'yellow';
		return 'red';
	}

	function lagClass(lag: number | null) {
		if (lag === null) return 'muted';
		if (lag < 50)  return 'green';
		if (lag < 200) return 'yellow';
		return 'red';
	}

	function fmtUptime(secs: number): string {
		const d = Math.floor(secs / 86400);
		const h = Math.floor((secs % 86400) / 3600);
		const m = Math.floor((secs % 3600) / 60);
		if (d > 0)  return `${d}d ${h}h`;
		if (h > 0)  return `${h}h ${m}m`;
		return `${m}m`;
	}
</script>

<h1>System Settings</h1>

<section>
	<h2>Chain Health</h2>
	<div class="card">
		{#if loading}
			<p class="muted">Loading…</p>
		{:else if chains.length === 0}
			<div class="table-wrap"><table>
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
			</table></div>
		{:else}
			<div class="table-wrap"><table>
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
			</table></div>
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
			<div class="table-wrap"><table>
				<thead><tr><th>User</th><th>Credentials</th></tr></thead>
				<tbody>
					{#each fido as f}
						<tr>
							<td class="mono">{f.user_id}</td>
							<td>{f.credential_count}</td>
						</tr>
					{/each}
				</tbody>
			</table></div>
		{/if}
	</div>
</section>

<section>
	<h2>Webhooks</h2>
	<div class="card">
		<p class="muted">
			Manage webhook endpoints and view delivery metrics in the
			<a href="/webhooks">Webhook Management</a> page.
		</p>
		<p class="muted">Events: <code>deposit_confirmed</code>, <code>withdrawal_broadcast</code>, <code>withdrawal_confirmed</code>, <code>withdrawal_failed</code>.</p>
	</div>
</section>

{#if detailedHealth}
<section>
	<h2>Detailed Chain Health</h2>
	<div class="chain-cards">
		{#each Object.entries(detailedHealth.chains) as [name, ch]}
			<div class="chain-card card">
				<div class="chain-header">
					<strong>{name.charAt(0).toUpperCase() + name.slice(1)}</strong>
					<span class="badge {statusClass(ch.status)}">{ch.status}</span>
				</div>
				<div class="kv">
					<span>Scanner block</span>
					<span class="mono muted">{ch.scanner_block ?? '—'}</span>
				</div>
				<div class="kv">
					<span>Lag</span>
					<span class="{lagClass(ch.scanner_lag)} mono">
						{ch.scanner_lag != null ? `${ch.scanner_lag} blocks` : '—'}
					</span>
				</div>
				<div class="kv">
					<span>Avg latency</span>
					<span class="mono muted">{ch.avg_rpc_latency_ms != null ? `${Math.round(ch.avg_rpc_latency_ms)}ms` : '—'}</span>
				</div>
				<div class="kv">
					<span>Deposits today</span>
					<span class="mono">{ch.deposits_today}</span>
				</div>
				<div class="kv">
					<span>Withdrawals today</span>
					<span class="mono">{ch.withdrawals_today}</span>
				</div>
			</div>
		{/each}
	</div>
</section>

<section>
	<h2>System Runtime</h2>
	<div class="card">
		{@const sys = detailedHealth.system}
		<div class="kv">
			<span>Uptime</span>
			<span class="val">{fmtUptime(sys.uptime_seconds)}</span>
		</div>
		<div class="kv">
			<span>DB pool</span>
			<span class="val">{sys.db_pool_active} / {sys.db_pool_size} active</span>
		</div>
		<div class="kv">
			<span>Memory</span>
			<span class="val">{sys.memory_mb > 0 ? `${sys.memory_mb} MB` : '—'}</span>
		</div>
		<div class="kv">
			<span>Pending webhooks</span>
			<span class="{sys.pending_webhooks > 0 ? 'val yellow' : 'val'}">{sys.pending_webhooks}</span>
		</div>
		<div class="kv">
			<span>Circuit breaker</span>
			<span class="badge {sys.circuit_breaker === 'normal' ? 'green' : 'red'}">{sys.circuit_breaker}</span>
		</div>
		<p class="note muted">Generated: {new Date(detailedHealth.generated_at).toLocaleString()}</p>
	</div>
</section>
{/if}

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

	.table-wrap { overflow-x: auto; -webkit-overflow-scrolling: touch; }
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

	a { color: var(--accent, #1f6feb); text-decoration: none; }
	a:hover { text-decoration: underline; }

	.chain-cards {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
		gap: 0.75rem;
	}

	.chain-card {
		padding: 0.9rem;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	.chain-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		margin-bottom: 0.3rem;
	}

	.green  { color: var(--green, #3fb950); }
	.yellow { color: var(--yellow, #d29922); }
	.red    { color: var(--red, #f85149); }
</style>
