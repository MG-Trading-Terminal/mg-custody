<script lang="ts">
	import { api } from '$lib/api';

	let step = $state<'intro' | 'show' | 'verify' | 'done'>('intro');
	let mnemonic = $state('');
	let words = $state<string[]>([]);
	let verifyAddress = $state('');
	let loading = $state(false);
	let error = $state('');

	// Verification
	let verifyInput = $state('');
	let verifyResult = $state<null | boolean>(null);

	// Backup status
	let backupStatus = $state<any>(null);

	async function loadStatus() {
		try {
			backupStatus = await api.getJson('/v1/admin/backup/status');
		} catch { /* ignore */ }
	}

	async function showMnemonic() {
		loading = true;
		error = '';
		try {
			const resp = await api.getJson<{
				mnemonic: string;
				verification_address: string;
			}>('/v1/admin/backup/mnemonic');
			mnemonic = resp.mnemonic;
			words = mnemonic.split(' ');
			verifyAddress = resp.verification_address;
			step = 'show';
		} catch (e) {
			error = e instanceof Error ? e.message : 'Failed to load mnemonic';
		} finally {
			loading = false;
		}
	}

	function goToVerify() {
		step = 'verify';
	}

	async function verifyPhrase() {
		loading = true;
		error = '';
		try {
			const resp = await api.post<{ valid: boolean }>('/v1/admin/backup/verify-mnemonic', {
				mnemonic: verifyInput.trim().toLowerCase(),
			});
			verifyResult = resp.valid;
			if (resp.valid) step = 'done';
		} catch (e) {
			error = e instanceof Error ? e.message : 'Verification failed';
		} finally {
			loading = false;
		}
	}

	import { onMount } from 'svelte';
	onMount(loadStatus);
</script>

<svelte:head><title>Backup — MG Custody</title></svelte:head>

<h1>Wallet Backup</h1>

{#if error}
	<div class="card err">{error}</div>
{/if}

{#if step === 'intro'}
	<div class="card intro">
		<div class="shield-icon">
			<svg width="48" height="48" viewBox="0 0 48 48" fill="none">
				<path d="M24 4L6 12v12c0 11 7.7 21.3 18 24 10.3-2.7 18-13 18-24V12L24 4z" fill="var(--accent)" opacity="0.15"/>
				<path d="M24 4L6 12v12c0 11 7.7 21.3 18 24 10.3-2.7 18-13 18-24V12L24 4z" stroke="var(--accent)" stroke-width="2" fill="none"/>
				<path d="M20 25l-4-4 2-2 2 2 6-6 2 2-8 8z" fill="var(--accent)"/>
			</svg>
		</div>
		<h2>Protect Your Funds</h2>
		<p>Your wallet is secured by a <strong>24-word recovery phrase</strong>. If you lose access to this server, these words are the ONLY way to recover your funds.</p>

		<div class="methods">
			<div class="method">
				<span class="method-icon">1</span>
				<div>
					<strong>24-Word Phrase</strong>
					<p>Write down and store in a safe (fireproof). Never photograph or store digitally.</p>
				</div>
			</div>
			<div class="method">
				<span class="method-icon">2</span>
				<div>
					<strong>Shamir Shares (3-of-5)</strong>
					<p>Split backup across 5 trusted people. Any 3 can restore. Use CLI: <code>custody-seed backup</code></p>
				</div>
			</div>
			<div class="method">
				<span class="method-icon">3</span>
				<div>
					<strong>Encrypted File</strong>
					<p>Already active — seed.enc on server. Protected by AES-256-GCM + PBKDF2.</p>
				</div>
			</div>
		</div>

		{#if backupStatus}
			<div class="status-row">
				<span>Seed loaded:</span>
				<span class="badge green">Active</span>
			</div>
			<div class="status-row">
				<span>First ETH address:</span>
				<code>{backupStatus.first_eth_address}</code>
			</div>
		{/if}

		<button class="btn-primary" onclick={showMnemonic} disabled={loading}>
			{loading ? 'Loading...' : 'Show Recovery Phrase'}
		</button>
		<p class="warn-text">Only view in a private, secure environment. Never share your screen.</p>
	</div>

{:else if step === 'show'}
	<div class="card mnemonic-card">
		<h2>Your 24-Word Recovery Phrase</h2>
		<p class="warn-box">Write these words down on paper. Do NOT screenshot or copy digitally.</p>

		<div class="word-grid">
			{#each words as word, i}
				<div class="word-cell">
					<span class="word-num">{i + 1}</span>
					<span class="word-text">{word}</span>
				</div>
			{/each}
		</div>

		<div class="verify-addr">
			<span>Verification address (ETH):</span>
			<code>{verifyAddress}</code>
		</div>

		<div class="actions">
			<button class="btn-primary" onclick={goToVerify}>I've Written It Down</button>
		</div>
	</div>

{:else if step === 'verify'}
	<div class="card">
		<h2>Verify Your Backup</h2>
		<p>Enter your 24 words to confirm you've saved them correctly:</p>

		<textarea
			class="verify-textarea"
			bind:value={verifyInput}
			placeholder="Enter all 24 words separated by spaces..."
			rows="4"
		></textarea>

		{#if verifyResult === false}
			<p class="err-text">Phrase does not match. Please try again.</p>
		{/if}

		<div class="actions">
			<button class="btn-secondary" onclick={() => { step = 'show'; verifyResult = null; }}>Show Again</button>
			<button class="btn-primary" onclick={verifyPhrase} disabled={loading || !verifyInput.trim()}>
				{loading ? 'Verifying...' : 'Verify'}
			</button>
		</div>
	</div>

{:else if step === 'done'}
	<div class="card done-card">
		<div class="done-icon">
			<svg width="64" height="64" viewBox="0 0 64 64" fill="none">
				<circle cx="32" cy="32" r="30" fill="var(--green)" opacity="0.15"/>
				<circle cx="32" cy="32" r="30" stroke="var(--green)" stroke-width="2" fill="none"/>
				<path d="M26 34l-6-6 3-3 3 3 11-11 3 3-14 14z" fill="var(--green)"/>
			</svg>
		</div>
		<h2>Backup Verified!</h2>
		<p>Your recovery phrase has been verified. Your funds are protected.</p>
		<div class="checklist">
			<div class="check-item"><span class="check">&#10003;</span> 24-word phrase written down</div>
			<div class="check-item"><span class="check">&#10003;</span> Phrase verified successfully</div>
			<div class="check-item"><span class="check">&#10003;</span> Encrypted seed file active</div>
		</div>
		<p class="next-step">Next: Run <code>custody-seed backup</code> to create Shamir shares for organizational backup.</p>
		<a href="/" class="btn-primary">Back to Dashboard</a>
	</div>
{/if}

<style>
	h1 { margin-top: 0; font-size: 1.5rem; }
	h2 { margin: 0 0 0.75rem; font-size: 1.15rem; }

	.card {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--r-lg);
		padding: var(--sp-6);
		margin-bottom: var(--sp-4);
		box-shadow: var(--shadow-1);
	}
	.err {
		color: var(--red);
		background: var(--red-soft);
		border: 1px solid rgba(248, 113, 113, 0.3);
		padding: var(--sp-3) var(--sp-4);
		border-radius: var(--r-md);
		font-size: 0.85rem;
		margin-bottom: var(--sp-4);
	}

	.intro { text-align: center; max-width: 600px; margin: 0 auto; }
	.shield-icon { margin-bottom: 1rem; }
	.intro p { color: var(--fg-muted); font-size: 0.9rem; line-height: 1.5; }

	.methods { text-align: left; margin: 1.5rem 0; display: flex; flex-direction: column; gap: 0.75rem; }
	.method {
		display: flex; gap: var(--sp-3); align-items: flex-start;
		padding: var(--sp-3) var(--sp-4);
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
	}
	.method-icon {
		display: flex; align-items: center; justify-content: center;
		width: 28px; height: 28px; border-radius: 50%;
		background: var(--accent-soft); color: var(--accent);
		font-weight: 700; font-size: 0.82rem;
		flex-shrink: 0;
	}
	.method strong { display: block; font-size: 0.9rem; margin-bottom: 0.15rem; }
	.method p { margin: 0; font-size: 0.8rem; color: var(--fg-muted); }
	.method code {
		background: var(--surface); padding: 1px 6px;
		border-radius: 4px; border: 1px solid var(--border);
		font-family: var(--font-mono); font-size: 0.76rem;
	}

	.status-row { display: flex; justify-content: space-between; align-items: center; padding: 0.5rem 0; font-size: 0.85rem; border-top: 1px solid var(--border); }
	.status-row:first-of-type { margin-top: 1rem; }
	.status-row code { font-size: 0.75rem; color: var(--fg-muted); }

	.badge { padding: 0.15rem 0.5rem; border-radius: 4px; font-size: 0.75rem; font-weight: 600; }
	.badge.green { background: rgba(63,185,80,0.15); color: var(--green); }

	.btn-primary, .btn-secondary {
		display: inline-flex; align-items: center; justify-content: center;
		gap: 6px;
		border-radius: var(--r-md);
		padding: 10px 18px;
		font-size: 0.9rem; font-weight: 600;
		cursor: pointer;
		text-decoration: none;
		border: 1px solid transparent;
		margin-top: var(--sp-3);
		transition: background 0.15s, border-color 0.15s;
	}
	.btn-primary { background: var(--accent); color: #0A0B0D; border-color: var(--accent); }
	.btn-primary:hover:not(:disabled) { background: var(--accent-strong); border-color: var(--accent-strong); }
	.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-secondary {
		background: transparent; color: var(--fg);
		border-color: var(--border);
		font-weight: 500;
	}
	.btn-secondary:hover { background: var(--hover-bg); border-color: var(--border-strong); }

	.warn-text { color: var(--yellow); font-size: 0.78rem; margin-top: 0.5rem; }

	/* Mnemonic display */
	.mnemonic-card { max-width: 650px; margin: 0 auto; }
	.warn-box {
		background: var(--red-soft); color: var(--red);
		border: 1px solid rgba(248, 113, 113, 0.3);
		border-radius: var(--r-md);
		padding: var(--sp-3) var(--sp-4);
		font-size: 0.85rem;
		margin-bottom: var(--sp-4);
		font-weight: 500;
	}

	.word-grid {
		display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px;
		margin: var(--sp-4) 0;
		padding: var(--sp-4);
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: var(--r-md);
	}
	.word-cell {
		display: flex; align-items: baseline; gap: 6px;
		padding: 8px 10px;
		background: var(--surface); border: 1px solid var(--border);
		border-radius: var(--r-sm);
	}
	.word-num {
		font-size: 0.68rem; color: var(--fg-dim);
		min-width: 1.4rem; font-weight: 500;
		font-family: var(--font-mono);
	}
	.word-text {
		font-weight: 500; font-size: 0.9rem;
		font-family: var(--font-mono);
		color: var(--fg-strong);
	}

	.verify-addr { margin: 1rem 0; padding: 0.5rem 0; border-top: 1px solid var(--border); font-size: 0.82rem; display: flex; justify-content: space-between; }
	.verify-addr code { font-size: 0.75rem; color: var(--fg-muted); }

	.actions { display: flex; gap: 0.75rem; justify-content: flex-end; margin-top: 1rem; }

	/* Verify */
	.verify-textarea {
		width: 100%;
		background: var(--surface-2); border: 1px solid var(--border);
		border-radius: var(--r-md);
		color: var(--fg);
		padding: var(--sp-3) var(--sp-4);
		font-size: 0.9rem;
		font-family: var(--font-mono);
		line-height: 1.6;
		resize: vertical;
		outline: none;
		transition: border-color 0.15s, box-shadow 0.15s;
	}
	.verify-textarea:focus {
		border-color: var(--accent);
		box-shadow: var(--shadow-focus);
		background: var(--surface);
	}
	.err-text { color: var(--red); font-size: 0.85rem; margin: 0.5rem 0 0; }

	/* Done */
	.done-card { text-align: center; max-width: 500px; margin: 0 auto; }
	.done-icon { margin-bottom: 1rem; }
	.checklist {
		text-align: left; margin: var(--sp-5) auto;
		max-width: 320px;
		display: flex; flex-direction: column; gap: 6px;
	}
	.check-item {
		display: flex; align-items: center; gap: 10px;
		padding: 6px 10px;
		font-size: 0.9rem;
		background: var(--green-soft);
		border: 1px solid rgba(52, 211, 153, 0.2);
		border-radius: var(--r-sm);
		color: var(--fg);
	}
	.check {
		display: inline-flex; align-items: center; justify-content: center;
		width: 18px; height: 18px; border-radius: 50%;
		background: var(--green); color: #0A0B0D;
		font-size: 0.7rem; font-weight: 700;
	}
	.next-step { color: var(--fg-muted); font-size: 0.82rem; margin-top: var(--sp-4); }
	.next-step code {
		background: var(--surface-2); border: 1px solid var(--border);
		padding: 2px 6px; border-radius: 3px;
		font-family: var(--font-mono); font-size: 0.78rem;
	}

	@media (max-width: 600px) {
		.word-grid { grid-template-columns: repeat(3, 1fr); }
	}
</style>
