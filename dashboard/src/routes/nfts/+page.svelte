<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api';
	import { t, locale } from '$lib/i18n';

	interface CustodiedNft {
		id: string;
		chain: string;
		contract_address: string;
		token_id: string;
		standard: string;
		owner_vault_id: string;
		metadata_uri: string | null;
		name: string | null;
		image_url: string | null;
		quantity: number;
		acquired_at: string;
	}

	let nfts    = $state<CustodiedNft[]>([]);
	let loading = $state(true);
	let error   = $state<string | null>(null);

	// Transfer modal state
	let transferNft     = $state<CustodiedNft | null>(null);
	let transferTo      = $state('');
	let transferQty     = $state(1);
	let transferError   = $state('');
	let transferPending = $state(false);
	let transferResult  = $state<{ status: string; message: string } | null>(null);

	// Register modal state
	let showRegister   = $state(false);
	let regChain       = $state('ethereum');
	let regContract    = $state('');
	let regTokenId     = $state('');
	let regStandard    = $state('erc721');
	let regVaultId     = $state('');
	let regName        = $state('');
	let regImageUrl    = $state('');
	let regQty         = $state(1);
	let regError       = $state('');
	let regPending     = $state(false);

	async function load() {
		loading = true;
		error   = null;
		try {
			const res = await api.getJson<{ count: number; nfts: CustodiedNft[] }>('/v1/nfts');
			nfts = res.nfts ?? [];
		} catch (e: unknown) {
			error = String(e);
		} finally {
			loading = false;
		}
	}

	function chainBadgeClass(chain: string): string {
		const map: Record<string, string> = {
			ethereum: 'chain-eth',
			solana:   'chain-sol',
			tron:     'chain-trx',
			bitcoin:  'chain-btc',
		};
		return map[chain.toLowerCase()] ?? 'chain-other';
	}

	function standardLabel(std: string): string {
		const map: Record<string, string> = {
			erc721:   'ERC-721',
			erc1155:  'ERC-1155',
			metaplex: 'Metaplex',
		};
		return map[std] ?? std.toUpperCase();
	}

	function fmt(ts: string) {
		return new Date(ts).toLocaleDateString();
	}

	function truncate(s: string, len = 10): string {
		if (s.length <= len * 2 + 3) return s;
		return `${s.slice(0, len)}…${s.slice(-6)}`;
	}

	function openTransfer(nft: CustodiedNft) {
		transferNft    = nft;
		transferTo     = '';
		transferQty    = 1;
		transferError  = '';
		transferResult = null;
	}

	function closeTransfer() {
		transferNft    = null;
		transferResult = null;
	}

	async function submitTransfer() {
		if (!transferNft) return;
		if (!transferTo.trim()) {
			transferError = 'Destination address is required.';
			return;
		}
		transferError   = '';
		transferPending = true;
		try {
			const body: Record<string, unknown> = { to_address: transferTo.trim() };
			if (transferNft.standard === 'erc1155') body.quantity = transferQty;
			transferResult = await api.post(`/v1/nfts/${transferNft.id}/transfer`, body);
		} catch (e: unknown) {
			transferError = String(e);
		} finally {
			transferPending = false;
		}
	}

	async function submitRegister() {
		regError = '';
		if (!regContract.trim()) { regError = 'Contract address is required.'; return; }
		if (!regTokenId.trim())  { regError = 'Token ID is required.'; return; }
		if (!regVaultId.trim())  { regError = 'Vault ID is required.'; return; }
		regPending = true;
		try {
			await api.post('/v1/nfts', {
				chain:            regChain,
				contract_address: regContract.trim(),
				token_id:         regTokenId.trim(),
				standard:         regStandard,
				owner_vault_id:   regVaultId.trim(),
				name:             regName.trim() || null,
				image_url:        regImageUrl.trim() || null,
				quantity:         regQty,
			});
			showRegister = false;
			await load();
		} catch (e: unknown) {
			regError = String(e);
		} finally {
			regPending = false;
		}
	}

	onMount(load);
</script>

<div class="page-header">
	<h1>{$locale, t('nfts.title')}</h1>
	<button class="btn-primary" onclick={() => (showRegister = true)}>+ Register NFT</button>
</div>

{#if loading}
	<p class="muted">Loading NFTs…</p>
{:else if error}
	<p class="error">Failed to load NFTs: {error}</p>
{:else if nfts.length === 0}
	<div class="empty-state">
		<p class="muted">No NFTs in custody.</p>
		<p class="muted small">Register an NFT to start managing it here.</p>
	</div>
{:else}
	<div class="nft-grid">
		{#each nfts as nft}
			<div class="nft-card">
				<div class="nft-image">
					{#if nft.image_url}
						<img src={nft.image_url} alt={nft.name ?? 'NFT'} loading="lazy" />
					{:else}
						<div class="nft-placeholder">
							<span class="placeholder-icon">&#9670;</span>
						</div>
					{/if}
				</div>
				<div class="nft-info">
					<div class="nft-name">{nft.name ?? `Token #${nft.token_id}`}</div>
					<div class="nft-meta">
						<span class="chain-badge {chainBadgeClass(nft.chain)}">{nft.chain}</span>
						<span class="std-badge">{standardLabel(nft.standard)}</span>
						{#if nft.quantity > 1}
							<span class="qty-badge">×{nft.quantity}</span>
						{/if}
					</div>
					<div class="nft-contract muted mono small">{truncate(nft.contract_address)}</div>
					<div class="nft-token-id muted small">#{nft.token_id}</div>
					<div class="nft-acquired muted small">Acquired {fmt(nft.acquired_at)}</div>
				</div>
				<div class="nft-actions">
					<button class="btn-sm" onclick={() => openTransfer(nft)}>Transfer</button>
				</div>
			</div>
		{/each}
	</div>
{/if}

<!-- Transfer modal -->
{#if transferNft}
<div class="modal-backdrop" role="dialog" aria-modal="true">
	<div class="modal">
		<h2>Transfer NFT</h2>
		<div class="transfer-nft-name">{transferNft.name ?? `Token #${transferNft.token_id}`}</div>

		{#if transferResult}
			<div class="success-box">
				<p class="green">Transfer queued successfully.</p>
				<p class="muted small">{transferResult.message}</p>
				<button class="btn-primary" onclick={closeTransfer}>Close</button>
			</div>
		{:else}
			<label>
				Destination Address
				<input
					type="text"
					bind:value={transferTo}
					placeholder="0x... or base58 address"
				/>
			</label>
			{#if transferNft.standard === 'erc1155'}
				<label>
					Quantity
					<input type="number" bind:value={transferQty} min="1" max={transferNft.quantity} />
				</label>
			{/if}
			{#if transferError}<p class="error">{transferError}</p>{/if}
			<div class="modal-actions">
				<button class="btn-primary" onclick={submitTransfer} disabled={transferPending}>
					{transferPending ? 'Submitting…' : 'Transfer'}
				</button>
				<button class="btn-sm" onclick={closeTransfer}>Cancel</button>
			</div>
		{/if}
	</div>
</div>
{/if}

<!-- Register modal -->
{#if showRegister}
<div class="modal-backdrop" role="dialog" aria-modal="true">
	<div class="modal">
		<h2>Register NFT</h2>
		<label>
			Chain
			<select bind:value={regChain}>
				<option value="ethereum">Ethereum</option>
				<option value="solana">Solana</option>
				<option value="tron">Tron</option>
			</select>
		</label>
		<label>
			Standard
			<select bind:value={regStandard}>
				{#if regChain === 'solana'}
					<option value="metaplex">Metaplex</option>
				{:else}
					<option value="erc721">ERC-721</option>
					<option value="erc1155">ERC-1155</option>
				{/if}
			</select>
		</label>
		<label>
			Contract Address
			<input type="text" bind:value={regContract} placeholder="0x... or mint address" />
		</label>
		<label>
			Token ID
			<input type="text" bind:value={regTokenId} placeholder="Decimal token ID" />
		</label>
		<label>
			Vault ID
			<input type="text" bind:value={regVaultId} placeholder="UUID of the owner vault" />
		</label>
		<label>
			Name (optional)
			<input type="text" bind:value={regName} placeholder="Bored Ape #1234" />
		</label>
		<label>
			Image URL (optional)
			<input type="text" bind:value={regImageUrl} placeholder="https://..." />
		</label>
		{#if regStandard === 'erc1155'}
			<label>
				Quantity
				<input type="number" bind:value={regQty} min="1" />
			</label>
		{/if}
		{#if regError}<p class="error">{regError}</p>{/if}
		<div class="modal-actions">
			<button class="btn-primary" onclick={submitRegister} disabled={regPending}>
				{regPending ? 'Registering…' : 'Register'}
			</button>
			<button class="btn-sm" onclick={() => (showRegister = false)}>Cancel</button>
		</div>
	</div>
</div>
{/if}

<style>
	.page-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		margin-bottom: 1.5rem;
	}
	h1 { margin: 0; font-size: 1.5rem; }

	.empty-state {
		padding: 3rem;
		text-align: center;
	}

	/* NFT grid */
	.nft-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
		gap: 1rem;
	}

	.nft-card {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		overflow: hidden;
		display: flex;
		flex-direction: column;
		transition: border-color 0.15s;
	}
	.nft-card:hover { border-color: var(--accent); }

	.nft-image {
		width: 100%;
		aspect-ratio: 1 / 1;
		background: var(--surface-2);
		overflow: hidden;
	}
	.nft-image img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}
	.nft-placeholder {
		width: 100%;
		height: 100%;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.placeholder-icon {
		font-size: 3rem;
		color: var(--fg-muted);
		opacity: 0.4;
	}

	.nft-info {
		padding: 0.75rem;
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}

	.nft-name {
		font-weight: 600;
		font-size: 0.9rem;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.nft-meta {
		display: flex;
		gap: 0.3rem;
		flex-wrap: wrap;
	}

	.chain-badge, .std-badge, .qty-badge {
		border-radius: 4px;
		padding: 0.1rem 0.4rem;
		font-size: 0.68rem;
		font-weight: 600;
		text-transform: uppercase;
	}

	.chain-eth   { background: rgba(98,126,234,.2);  color: #627eea; }
	.chain-sol   { background: rgba(153,69,255,.2);  color: #9945ff; }
	.chain-trx   { background: rgba(239,68,68,.2);   color: #ef4444; }
	.chain-btc   { background: rgba(247,147,26,.2);  color: #f7931a; }
	.chain-other { background: rgba(139,148,158,.2); color: var(--fg-muted); }

	.std-badge  { background: rgba(88,166,255,.12); color: var(--accent); }
	.qty-badge  { background: rgba(63,185,80,.12);  color: var(--green); }

	.nft-contract { word-break: break-all; }

	.nft-actions {
		padding: 0.5rem 0.75rem 0.75rem;
	}

	/* Buttons */
	.btn-primary {
		background: var(--accent);
		color: #fff;
		border: none;
		border-radius: var(--r-sm);
		padding: 0.4rem 0.9rem;
		font-size: 0.85rem;
		cursor: pointer;
	}
	.btn-primary:hover { filter: brightness(1.1); }
	.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-sm {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		padding: 0.25rem 0.7rem;
		font-size: 0.8rem;
		cursor: pointer;
		color: inherit;
		width: 100%;
	}
	.btn-sm:hover { border-color: var(--accent); }

	/* Modals */
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
		width: 460px;
		max-width: 95vw;
		display: flex;
		flex-direction: column;
		gap: 0.8rem;
		max-height: 90vh;
		overflow-y: auto;
	}

	.modal h2 { margin: 0; font-size: 1.1rem; }
	.transfer-nft-name { font-weight: 600; color: var(--accent); }

	.modal label {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		font-size: 0.85rem;
		color: var(--fg-muted);
	}

	.modal input[type="text"],
	.modal input[type="number"],
	.modal select {
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-sm);
		padding: 0.45rem 0.75rem;
		color: inherit;
		font-size: 0.9rem;
	}

	.modal-actions { display: flex; gap: 0.5rem; justify-content: flex-end; margin-top: 0.5rem; }

	.success-box { display: flex; flex-direction: column; gap: 0.5rem; }

	/* Utilities */
	.muted  { color: var(--fg-muted); }
	.mono   { font-family: var(--font-mono); font-variant-numeric: tabular-nums; }
	.small  { font-size: 0.78rem; }
	.green  { color: var(--green); }
	.error  { color: var(--red); font-size: 0.82rem; margin: 0; }
</style>
