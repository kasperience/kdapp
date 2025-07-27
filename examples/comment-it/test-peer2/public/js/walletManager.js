import { resilientFetch, typewriterEffect, truncateKaspaAddress } from './utils.js';

export let currentWallet = null;

export function showCreateWallet() {
    document.getElementById('createWalletSection').style.display = 'block';
    document.getElementById('importWalletSection').style.display = 'none';
    document.getElementById('proceedNewButton').addEventListener('click', proceedWithWallet);
    document.getElementById('copyKeyButton').addEventListener('click', copyPrivateKey);
}

export function showImportWallet() {
    document.getElementById('createWalletSection').style.display = 'none';
    document.getElementById('importWalletSection').style.display = 'block';
    document.getElementById('importButton').addEventListener('click', validateAndImportWallet);
}

export async function generateNewWallet() {
    const button = document.getElementById('generateButton');
    const originalText = button.textContent;
    button.textContent = '[ GENERATING... ]';
    button.disabled = true;
    
    try {
        // Generate a random 32-byte private key
        const privateKeyBytes = new Uint8Array(32);
        crypto.getRandomValues(privateKeyBytes);
        const privateKeyHex = Array.from(privateKeyBytes)
            .map(b => b.toString(16).padStart(2, '0'))
            .join('');
        
        // Display the private key
        document.getElementById('generatedPrivateKey').value = privateKeyHex;
        document.getElementById('copyKeyButton').disabled = false;
        document.getElementById('proceedNewButton').style.display = 'block';
        
        // Store temporarily for use
        currentWallet = { privateKey: privateKeyHex, wasCreated: true };
        
        button.textContent = '[ WALLET GENERATED ]';
        button.style.background = 'var(--success)';
        button.style.borderColor = 'var(--success)';
        
        typewriterEffect('WALLET GENERATED! COPY YOUR PRIVATE KEY IMMEDIATELY!', button.parentElement);
        
    } catch (error) {
        console.error('Wallet generation failed:', error);
        button.textContent = originalText;
        button.disabled = false;
        typewriterEffect('ERROR: Failed to generate wallet', button.parentElement);
    }
}

export function copyPrivateKey() {
    const privateKey = document.getElementById('generatedPrivateKey').value;
    navigator.clipboard.writeText(privateKey).then(() => {
        const button = document.getElementById('copyKeyButton');
        const originalText = button.textContent;
        button.textContent = '✅ COPIED';
        button.style.background = 'var(--success)';
        
        setTimeout(() => {
            button.textContent = originalText;
            button.style.background = 'var(--primary-teal)';
        }, 2000);
    }).catch(err => {
        console.error('Failed to copy: ', err);
        alert('Failed to copy private key. Please select and copy manually.');
    });
}

export async function validateAndImportWallet() {
    const button = document.getElementById('importButton');
    const originalText = button.textContent;
    const privateKeyInput = document.getElementById('importPrivateKey');
    const privateKey = privateKeyInput.value.trim();
    
    button.textContent = '[ VALIDATING... ]';
    button.disabled = true;
    
    try {
        // Validate private key format (64 hex characters)
        if (!/^[0-9a-fA-F]{64}$/.test(privateKey)) {
            throw new Error('Invalid private key format. Must be 64 hexadecimal characters.');
        }
        
        // Store the imported wallet
        currentWallet = { privateKey: privateKey, wasCreated: false };
        
        button.textContent = '[ WALLET IMPORTED ]';
        button.style.background = 'var(--success)';
        button.style.borderColor = 'var(--success)';
        
        // Proceed to authentication
        await proceedWithWallet();
        
    } catch (error) {
        console.error('Wallet import failed:', error);
        button.textContent = originalText;
        button.disabled = false;
        typewriterEffect(`IMPORT ERROR: ${error.message}`, button.parentElement);
        privateKeyInput.style.borderColor = 'var(--error)';
        
        setTimeout(() => {
            privateKeyInput.style.borderColor = 'var(--primary-teal)';
        }, 3000);
    }
}

export async function proceedWithWallet() {
    if (!currentWallet || !currentWallet.privateKey) {
        alert('No wallet available. Please create or import a wallet first.');
        return;
    }
    
    try {
        // Check if user wants to save to file
        const saveToFile = currentWallet.wasCreated ? 
            document.getElementById('saveToFileCheck').checked :
            document.getElementById('saveImportedToFileCheck').checked;
        
        if (saveToFile) {
            // Send the private key to backend for storage
            const response = await resilientFetch('/wallet-participant', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    private_key: currentWallet.privateKey,
                    save_to_file: true
                })
            });
            
            const data = await response.json();
            if (data.error) {
                throw new Error(data.error);
            }
            
            currentWallet.kaspaAddress = data.kaspa_address;
            currentWallet.publicKey = data.public_key;
        } else {
            // Use the wallet without saving to file (more secure)
            const response = await resilientFetch('/wallet-participant', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    private_key: currentWallet.privateKey,
                    save_to_file: false
                })
                
            });
            
            const data = await response.json();
            if (data.error) {
                throw new Error(data.error);
            }
            
            currentWallet.kaspaAddress = data.kaspa_address;
            currentWallet.publicKey = data.public_key;
        }
        
        // Show authentication panel
        showAuthPanel();
        
    } catch (error) {
        console.error('Wallet setup failed:', error);
        typewriterEffect(`SETUP ERROR: ${error.message}`, document.getElementById('walletPanel'));
    }
}

export function showAuthPanel() {
    // Hide wallet panel
    document.getElementById('walletPanel').style.display = 'none';
    
    // Show auth panel
    document.getElementById('authPanel').style.display = 'block';
    document.getElementById('authButton').addEventListener('click', connectWallet);
    
    // Update active wallet display with truncated address
    const truncatedAddress = truncateKaspaAddress(currentWallet.kaspaAddress);
    document.getElementById('activeWalletAddress').textContent = truncatedAddress;
    document.getElementById('walletAddress').textContent = truncatedAddress;
    
    // Show funding info if wallet was just created
    if (currentWallet.wasCreated) {
        showFundingInfo(currentWallet.kaspaAddress);
    }
}

export function changeWallet() {
    // Reset state
    currentWallet = null;
    window.currentEpisodeId = null;
    window.currentSessionToken = null;
    window.isAuthenticated = false;
    
    // Show wallet panel again
    document.getElementById('walletPanel').style.display = 'block';
    document.getElementById('authPanel').style.display = 'none';
    document.getElementById('commentForm').style.display = 'none';
    
    // Reset wallet sections
    document.getElementById('createWalletSection').style.display = 'none';
    document.getElementById('importWalletSection').style.display = 'none';
    
    // Reset forms
    document.getElementById('generatedPrivateKey').value = '';
    document.getElementById('importPrivateKey').value = '';
    document.getElementById('saveToFileCheck').checked = false;
    document.getElementById('saveImportedToFileCheck').checked = false;
    
    // Reset buttons
    const generateButton = document.getElementById('generateButton');
    generateButton.textContent = '[ GENERATE NEW WALLET ]';
    generateButton.style.background = 'transparent';
    generateButton.style.borderColor = 'var(--primary-teal)';
    generateButton.disabled = false;
    
    const importButton = document.getElementById('importButton');
    importButton.textContent = '[ VALIDATE & IMPORT WALLET ]';
    importButton.style.background = 'transparent';
    importButton.style.borderColor = 'var(--primary-teal)';
    importButton.disabled = false;
    
    document.getElementById('copyKeyButton').disabled = true;
    document.getElementById('proceedNewButton').style.display = 'none';
}

export async function checkExistingWallet() {
    try {
        const response = await resilientFetch('/wallet/status');
        const data = await response.json();
        
        if (data.exists && !data.error) {
            // User has an existing wallet
            currentWallet = {
                privateKey: 'stored_in_file', // Placeholder
                kaspaAddress: data.kaspa_address,
                publicKey: 'from_file', // Will be fetched when needed
                wasCreated: data.was_created,
                needsFunding: data.needs_funding
            };
            
            showAuthPanel();
        } else {
            // No existing wallet, show wallet setup
            document.getElementById('walletPanel').style.display = 'block';
            document.getElementById('authPanel').style.display = 'none';
        }
    } catch (error) {
        console.error('Failed to check existing wallet:', error);
        // Show wallet setup on error
        document.getElementById('walletPanel').style.display = 'block';
        document.getElementById('authPanel').style.display = 'none';
    }
}

// Show funding information for new wallets
export function showFundingInfo(kaspaAddress) { // Added comment to force refresh
    const fundingDiv = document.createElement('div');
    fundingDiv.style.marginTop = '20px';
    fundingDiv.style.padding = '15px';
    fundingDiv.style.background = 'var(--warning)';
    fundingDiv.style.color = 'var(--bg-black)';
    fundingDiv.style.borderRadius = '4px';
    fundingDiv.innerHTML = `
        <strong>NEW WALLET CREATED - FUNDING REQUIRED</strong><br>
        <small>Address: ${kaspaAddress}</small><br>
        <small>Get testnet funds: <a href="https://faucet.kaspanet.io/" target="_blank" style="color: var(--bg-black);">https://faucet.kaspanet.io/</a></small><br>
        <small>Refresh page after funding to continue authentication.</small>
    `;
    
    document.querySelector('.auth-panel').appendChild(fundingDiv);
}
