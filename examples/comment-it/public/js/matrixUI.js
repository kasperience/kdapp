// Matrix rain effect
export function createMatrixRain() {
    const matrix = document.getElementById('matrixBg');
    const characters = '01アイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワヲン';
    
    for (let i = 0; i < 50; i++) {
        const column = document.createElement('div');
        column.className = 'matrix-column';
        column.style.left = Math.random() * 100 + '%';
        column.style.animationDuration = (Math.random() * 15 + 10) + 's';
        column.style.animationDelay = Math.random() * 10 + 's';
        
        let text = '';
        for (let j = 0; j < 50; j++) {
            text += characters[Math.floor(Math.random() * characters.length)] + '<br>';
        }
        column.innerHTML = text;
        matrix.appendChild(column);
    }
}

// Simulate block height updates
export function initBlockHeightUpdater() {
    setInterval(() => {
        const blockHeight = document.getElementById('blockHeight');
        const currentHeight = parseInt(blockHeight.textContent.replace(/,/g, ''));
        blockHeight.textContent = (currentHeight + 1).toLocaleString();
    }, 5000);
}

// Konami code easter egg
export function initKonamiCode() {
    let konamiCode = [];
    const konamiPattern = ['ArrowUp', 'ArrowUp', 'ArrowDown', 'ArrowDown', 'ArrowLeft', 'ArrowRight', 'ArrowLeft', 'ArrowRight', 'b', 'a'];
    
    document.addEventListener('keydown', (e) => {
        konamiCode.push(e.key);
        konamiCode = konamiCode.slice(-10);
        
        if (konamiCode.join(',') === konamiPattern.join(',')) {
            document.body.style.animation = 'glow-pulse 0.5s infinite';
            alert('EPISODE HACK MODE ACTIVATED!');
        }
    });
}
