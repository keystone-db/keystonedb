// KeystoneDB Notebook JavaScript

let currentNotebook = null;
let websocket = null;
let cellCounter = 0;

// Initialize the notebook interface
document.addEventListener('DOMContentLoaded', () => {
    initializeWebSocket();
    loadNotebooks();
    bindEventHandlers();
    createNewNotebook();
});

// Initialize WebSocket connection
function initializeWebSocket() {
    const wsUrl = `ws://${window.location.host}/ws`;
    websocket = new WebSocket(wsUrl);

    websocket.onopen = () => {
        console.log('WebSocket connected');
    };

    websocket.onmessage = (event) => {
        const response = JSON.parse(event.data);
        handleWebSocketResponse(response);
    };

    websocket.onerror = (error) => {
        console.error('WebSocket error:', error);
    };

    websocket.onclose = () => {
        console.log('WebSocket disconnected');
        // Attempt to reconnect after 1 second
        setTimeout(initializeWebSocket, 1000);
    };
}

// Handle WebSocket responses
function handleWebSocketResponse(response) {
    switch (response.type) {
        case 'Result':
            displayQueryResult(response.id, response.rows, response.execution_time_ms);
            break;
        case 'Error':
            displayQueryError(response.id, response.message);
            break;
        case 'Progress':
            displayQueryProgress(response.id, response.message);
            break;
    }
}

// Bind event handlers
function bindEventHandlers() {
    document.getElementById('new-notebook-btn').addEventListener('click', createNewNotebook);
    document.getElementById('save-notebook-btn').addEventListener('click', saveNotebook);
    document.getElementById('open-notebook-btn').addEventListener('click', loadNotebooks);

    document.getElementById('notebook-title').addEventListener('change', (e) => {
        if (currentNotebook) {
            currentNotebook.title = e.target.value;
        }
    });
}

// Load notebooks list
async function loadNotebooks() {
    try {
        const response = await fetch('/api/notebooks');
        const notebooks = await response.json();

        const listElement = document.getElementById('notebook-list');
        listElement.innerHTML = '';

        notebooks.forEach(nb => {
            const item = document.createElement('div');
            item.className = 'notebook-item';
            item.textContent = nb.title;
            item.onclick = () => openNotebook(nb.id);
            listElement.appendChild(item);
        });
    } catch (error) {
        console.error('Failed to load notebooks:', error);
    }
}

// Create a new notebook
async function createNewNotebook() {
    try {
        const response = await fetch('/api/notebooks', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ title: 'Untitled Notebook' })
        });

        currentNotebook = await response.json();
        displayNotebook(currentNotebook);
        loadNotebooks(); // Refresh list
    } catch (error) {
        console.error('Failed to create notebook:', error);
    }
}

// Open an existing notebook
async function openNotebook(id) {
    try {
        const response = await fetch(`/api/notebooks/${id}`);
        currentNotebook = await response.json();
        displayNotebook(currentNotebook);

        // Update active state in sidebar
        document.querySelectorAll('.notebook-item').forEach(item => {
            item.classList.remove('active');
            if (item.textContent === currentNotebook.title) {
                item.classList.add('active');
            }
        });
    } catch (error) {
        console.error('Failed to open notebook:', error);
    }
}

// Display a notebook
function displayNotebook(notebook) {
    document.getElementById('notebook-title').value = notebook.title;

    const container = document.getElementById('cells-container');
    container.innerHTML = '';

    notebook.cells.forEach(cell => {
        displayCell(cell);
    });
}

// Save the current notebook
async function saveNotebook() {
    if (!currentNotebook) return;

    try {
        const cells = [];
        document.querySelectorAll('.cell').forEach(cellElement => {
            const cellId = cellElement.dataset.cellId;
            const content = cellElement.querySelector('.cell-input').value;
            const type = cellElement.querySelector('.cell-type-label').textContent.toLowerCase();

            cells.push({
                id: cellId,
                cell_type: type,
                content: content,
                outputs: []
            });
        });

        const response = await fetch(`/api/notebooks/${currentNotebook.id}`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({
                title: document.getElementById('notebook-title').value,
                cells: cells
            })
        });

        if (response.ok) {
            console.log('Notebook saved');
        }
    } catch (error) {
        console.error('Failed to save notebook:', error);
    }
}

// Add a new cell
function addCell(type) {
    const cell = {
        id: `cell-${++cellCounter}`,
        cell_type: type,
        content: '',
        outputs: []
    };

    displayCell(cell);

    // Add to current notebook if exists
    if (currentNotebook) {
        if (!currentNotebook.cells) {
            currentNotebook.cells = [];
        }
        currentNotebook.cells.push(cell);
    }
}

// Display a cell
function displayCell(cell) {
    const template = document.getElementById('cell-template');
    const cellElement = template.content.cloneNode(true);

    const cellDiv = cellElement.querySelector('.cell');
    cellDiv.dataset.cellId = cell.id;

    const typeLabel = cellElement.querySelector('.cell-type-label');
    typeLabel.textContent = cell.cell_type.toUpperCase();

    const input = cellElement.querySelector('.cell-input');
    input.value = cell.content || '';

    if (cell.cell_type === 'markdown') {
        input.placeholder = 'Enter markdown...';
    } else if (cell.cell_type === 'chart') {
        input.placeholder = 'Enter chart spec (JSON)...';
    }

    // Display outputs if any
    if (cell.outputs && cell.outputs.length > 0) {
        const output = cellElement.querySelector('.cell-output');
        cell.outputs.forEach(out => {
            displayOutput(output, out);
        });
    }

    document.getElementById('cells-container').appendChild(cellElement);
}

// Execute a cell
async function executeCell(button) {
    const cell = button.closest('.cell');
    const cellId = cell.dataset.cellId;
    const content = cell.querySelector('.cell-input').value;
    const type = cell.querySelector('.cell-type-label').textContent.toLowerCase();

    // Clear previous output
    const output = cell.querySelector('.cell-output');
    output.innerHTML = '<div class="loading"></div>';

    if (type === 'query') {
        // Execute via WebSocket for real-time results
        if (websocket && websocket.readyState === WebSocket.OPEN) {
            websocket.send(JSON.stringify({
                type: 'Execute',
                id: cellId,
                query: content
            }));
        } else {
            output.innerHTML = '<div class="output-error">WebSocket not connected</div>';
        }
    } else {
        // Execute via HTTP API for other cell types
        try {
            const response = await fetch(`/api/notebooks/${currentNotebook.id}/cells/${cellId}/execute`, {
                method: 'POST'
            });

            const result = await response.json();
            displayOutput(output, result);
        } catch (error) {
            output.innerHTML = `<div class="output-error">Error: ${error.message}</div>`;
        }
    }
}

// Display query result
function displayQueryResult(cellId, rows, executionTime) {
    const cell = document.querySelector(`[data-cell-id="${cellId}"]`);
    if (!cell) return;

    const output = cell.querySelector('.cell-output');

    if (rows.length === 0) {
        output.innerHTML = `<div class="output-success">No results (${executionTime}ms)</div>`;
        return;
    }

    // Create table
    const table = document.createElement('table');
    table.className = 'output-table';

    // Header
    const thead = document.createElement('thead');
    const headerRow = document.createElement('tr');
    const keys = Object.keys(rows[0]);
    keys.forEach(key => {
        const th = document.createElement('th');
        th.textContent = key;
        headerRow.appendChild(th);
    });
    thead.appendChild(headerRow);
    table.appendChild(thead);

    // Body
    const tbody = document.createElement('tbody');
    rows.forEach(row => {
        const tr = document.createElement('tr');
        keys.forEach(key => {
            const td = document.createElement('td');
            const value = row[key];
            td.textContent = typeof value === 'object' ? JSON.stringify(value) : value;
            tr.appendChild(td);
        });
        tbody.appendChild(tr);
    });
    table.appendChild(tbody);

    output.innerHTML = '';
    output.appendChild(table);

    // Add execution time
    const timeDiv = document.createElement('div');
    timeDiv.className = 'output-success';
    timeDiv.textContent = `${rows.length} rows (${executionTime}ms)`;
    output.appendChild(timeDiv);
}

// Display query error
function displayQueryError(cellId, message) {
    const cell = document.querySelector(`[data-cell-id="${cellId}"]`);
    if (!cell) return;

    const output = cell.querySelector('.cell-output');
    output.innerHTML = `<div class="output-error">${message}</div>`;
}

// Display query progress
function displayQueryProgress(cellId, message) {
    const cell = document.querySelector(`[data-cell-id="${cellId}"]`);
    if (!cell) return;

    const output = cell.querySelector('.cell-output');
    output.innerHTML = `<div class="loading"></div> ${message}`;
}

// Display output based on type
function displayOutput(outputElement, result) {
    outputElement.innerHTML = '';

    if (result.type === 'Result') {
        displayQueryResult(outputElement.closest('.cell').dataset.cellId, result.rows, result.execution_time_ms);
    } else if (result.type === 'Error') {
        outputElement.innerHTML = `<div class="output-error">${result.message}</div>`;
    } else if (result.type === 'Markdown') {
        outputElement.innerHTML = result.html;
    } else if (result.type === 'Chart') {
        // Placeholder for chart rendering
        outputElement.innerHTML = '<div>Chart rendering not yet implemented</div>';
    }
}

// Delete a cell
function deleteCell(button) {
    const cell = button.closest('.cell');
    const cellId = cell.dataset.cellId;

    // Remove from DOM
    cell.remove();

    // Remove from current notebook
    if (currentNotebook && currentNotebook.cells) {
        currentNotebook.cells = currentNotebook.cells.filter(c => c.id !== cellId);
    }
}