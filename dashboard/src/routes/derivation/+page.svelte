<script lang="ts">
	import { api } from '$lib/api';

	interface DerivedAddress {
		index: number;
		path: string;
		address: string;
		used: boolean;
	}

	interface DerivationTree {
		chain: string;
		base_path: string;
		depth: number;
		addresses: DerivedAddress[];
	}

	const CHAINS = ['bitcoin', 'ethereum', 'tron', 'solana'];

	let chain   = $state('ethereum');
	let depth   = $state(10);
	let tree    = $state<DerivationTree | null>(null);
	let loading = $state(false);
	let error   = $state('');
	let verifyResult = $state<string | null>(null);
	let verifyingIdx = $state<number | null>(null);

	async function loadTree() {
		loading = true;
		error   = '';
		tree    = null;
		try {
			tree = await api.getJson<DerivationTree>(
				`/v1/admin/derivation-tree/${chain}?depth=${depth}`
			);
		} catch (e: unknown) {
			error = String(e);
		}
		loading = false;
	}

	async function verifyAddress(idx: number, expected: string) {
		verifyingIdx  = idx;
		verifyResult  = null;
		try {
			const res = await api.getJson<DerivationTree>(
				`/v1/admin/derivation-tree/${chain}?depth=${idx + 1}`
			);
			const addr = res.addresses.find(a => a.index === idx);
			if (addr && addr.address === expected) {
				verifyResult = `Index ${idx}: address matches — ${expected}`;
			} else {
				verifyResult = `Index ${idx}: MISMATCH — expected ${expected}, got ${addr?.address ?? 'not found'}`;
			}
		} catch (e: unknown) {
			verifyResult = `Verify failed: ${String(e)}`;
		}
		verifyingIdx = null;
	}

	function copyToClipboard(text: string) {
		navigator.clipboard.writeText(text).catch(() => {});
	}
</script>

<h1>HD Derivation Path Explorer</h1>

<div class="controls card">
	<label>
		Chain
		<select bind:value={chain}>
			{#each CHAINS as c}
				<option value={c}>{c.charAt(0).toUpperCase() + c.slice(1)}</option>
			{/each}
		</select>
	</label>

	<label>
		Depth <span class="muted">(1–100)</span>
		<input type="number" bind:value={depth} min="1" max="100" />
	</label>

	<button class="btn-primary" onclick={loadTree} disabled={loading}>
		{loading ? 'Loading…' : 'Derive'}
	</button>
</div>

{#if error}
<p class="error">{error}</p>
{/if}

{#if verifyResult}
<div class="verify-result card {verifyResult.includes('MISMATCH') ? 'red-border' : 'green-border'}">
	{verifyResult}
	<button class="btn-sm" onclick={() => (verifyResult = null)}>Dismiss</button>
</div>
{/if}

{#if tree}
<section>
	<div class="tree-header">
		<div>
			<h2>
				{tree.chain.charAt(0).toUpperCase() + tree.chain.slice(1)}
				<span class="muted">— {tree.addresses.length} addresses</span>
			</h2>
			<span class="mono muted">Base path: {tree.base_path}</span>
		</div>
	</div>

	<div class="card">
		<div class="table-wrap"><table>
			<thead>
				<tr>
					<th>#</th>
					<th>Path</th>
					<th>Address</th>
					<th>Status</th>
					<th>Actions</th>
				</tr>
			</thead>
			<tbody>
				{#each tree.addresses as a}
					<tr>
						<td class="mono muted">{a.index}</td>
						<td class="mono path">{a.path}</td>
						<td>
							<span class="mono addr">{a.address}</span>
							<button
								class="btn-icon"
								onclick={() => copyToClipboard(a.address)}
								title="Copy address"
							>⧉</button>
						</td>
						<td>
							{#if a.used}
								<span class="badge green">used</span>
							{:else}
								<span class="badge grey">unused</span>
							{/if}
						</td>
						<td>
							<button
								class="btn-sm"
								onclick={() => verifyAddress(a.index, a.address)}
								disabled={verifyingIdx === a.index}
							>
								{verifyingIdx === a.index ? '…' : 'Verify'}
							</button>
						</td>
					</tr>
				{/each}
			</tbody>
		</table></div>
	</div>
</section>
{/if}

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }
	h2 { font-size: 1rem; color: inherit; margin: 0; }

	.controls {
		display: flex;
		align-items: flex-end;
		gap: 1rem;
		margin-bottom: 1rem;
		flex-wrap: wrap;
	}

	.controls label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.85rem;
		color: var(--fg-muted, #8b949e);
	}

	.controls select,
	.controls input[type="number"] {
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		padding: 0.4rem 0.7rem;
		color: inherit;
		font-size: 0.9rem;
	}

	.card {
		background: var(--card-bg, #161b22);
		border: 1px solid var(--border, #30363d);
		border-radius: 8px;
		padding: 1rem;
	}

	.green-border { border-color: var(--green, #3fb950); }
	.red-border   { border-color: var(--red,   #f85149); }

	.verify-result {
		display: flex;
		align-items: center;
		gap: 1rem;
		font-size: 0.88rem;
		margin-bottom: 1rem;
	}

	section { margin-bottom: 1rem; }

	.tree-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 0.5rem;
	}

	.table-wrap { overflow-x: auto; }
	table { width: 100%; border-collapse: collapse; font-size: 0.88rem; }
	th { text-align: left; padding: 0.5rem 0.75rem; color: var(--fg-muted, #8b949e); border-bottom: 1px solid var(--border, #30363d); }
	td { padding: 0.45rem 0.75rem; border-bottom: 1px solid var(--border, #30363d); vertical-align: middle; }

	.badge { border-radius: 4px; padding: 0.15rem 0.45rem; font-size: 0.72rem; font-weight: 600; }
	.badge.green { background: rgba(63,185,80,.15);  color: var(--green,  #3fb950); }
	.badge.grey  { background: rgba(139,148,158,.1); color: var(--fg-muted, #8b949e); }

	.mono { font-family: monospace; font-size: 0.85rem; }
	.path { color: var(--yellow, #d29922); }
	.addr { word-break: break-all; }
	.muted { color: var(--fg-muted, #8b949e); }

	.btn-primary {
		background: var(--accent, #1f6feb);
		color: #fff;
		border: none;
		border-radius: 6px;
		padding: 0.42rem 1rem;
		font-size: 0.88rem;
		cursor: pointer;
		align-self: flex-end;
	}
	.btn-primary:hover    { filter: brightness(1.1); }
	.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-sm {
		background: var(--hover-bg, #1c2128);
		border: 1px solid var(--border, #30363d);
		border-radius: 6px;
		padding: 0.22rem 0.55rem;
		font-size: 0.78rem;
		cursor: pointer;
		color: inherit;
	}
	.btn-sm:hover    { border-color: var(--accent, #1f6feb); }
	.btn-sm:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-icon {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--fg-muted, #8b949e);
		font-size: 0.85rem;
		padding: 0 0.25rem;
		vertical-align: middle;
	}
	.btn-icon:hover { color: var(--accent, #1f6feb); }

	.error { color: var(--red, #f85149); font-size: 0.88rem; }
</style>
