<script lang="ts">
	const CHAINS = [
		{ name: 'Bitcoin',  rpc: 'bitcoind @ localhost:8332',    confirmations: 3  },
		{ name: 'Ethereum', rpc: 'execution node @ :8545',       confirmations: 12 },
		{ name: 'Tron',     rpc: 'tron-grid @ :8090',            confirmations: 20 },
		{ name: 'Solana',   rpc: 'validator RPC @ :8899',        confirmations: 0  },
	];

	const CIRCUIT = [
		{ label: 'Per-transaction cap', value: '$500,000' },
		{ label: 'Hourly outflow limit', value: '$10,000,000' },
		{ label: 'Daily outflow limit',  value: '$50,000,000' },
	];
</script>

<h1>System Settings</h1>

<section>
	<h2>Chain Configuration</h2>
	<div class="card">
		<table>
			<thead>
				<tr><th>Chain</th><th>RPC Endpoint</th><th>Confirmations</th></tr>
			</thead>
			<tbody>
				{#each CHAINS as c}
					<tr>
						<td>{c.name}</td>
						<td class="mono muted">{c.rpc}</td>
						<td>{c.confirmations === 0 ? 'finalized' : c.confirmations}</td>
					</tr>
				{/each}
			</tbody>
		</table>
		<p class="note muted">Override via <code>CUSTODY_CHAINS__*</code> environment variables.</p>
	</div>
</section>

<section>
	<h2>Circuit Breaker</h2>
	<div class="card">
		{#each CIRCUIT as row}
			<div class="kv">
				<span>{row.label}</span>
				<span class="val">{row.value}</span>
			</div>
		{/each}
		<p class="note muted">Trips fire a PagerDuty/webhook alert. State is persisted in PostgreSQL.</p>
	</div>
</section>

<section>
	<h2>Webhooks</h2>
	<div class="card empty">
		<p class="muted">Webhook endpoint configuration coming soon.</p>
		<p class="muted">Events: <code>deposit_confirmed</code>, <code>withdrawal_broadcast</code>, <code>withdrawal_confirmed</code>, <code>withdrawal_failed</code>.</p>
	</div>
</section>

<section>
	<h2>API Keys</h2>
	<div class="card empty">
		<p class="muted">HMAC-SHA256 key management coming soon.</p>
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

	.mono  { font-family: monospace; font-size: 0.85rem; }
	.muted { color: var(--fg-muted, #8b949e); }

	.kv {
		display: flex;
		justify-content: space-between;
		padding: 0.45rem 0;
		border-bottom: 1px solid var(--border, #30363d);
		font-size: 0.9rem;
	}

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
