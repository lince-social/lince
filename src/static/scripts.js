// scripts.js
let typingInterval;

// Function to simulate typing into an input field
function simulateTyping(input) {
  let value = input.value;
  input.value = ''; // Clear input first
  let index = 0;

  typingInterval = setInterval(() => {
    if (index < value.length) {
      input.value += value[index++];
    } else {
      clearInterval(typingInterval); // Stop when finished typing
    }
  }, 100); // Typing speed, adjust as needed (100ms per character)
}

// Function to check if any input is focused, and if not, focus the last input field
function checkFocusAndAutotype() {
  const focusedElement = document.activeElement;

  // Check if the focused element is an input (it might be another element like a button)
  if (!focusedElement || focusedElement.tagName !== 'INPUT') {
    // Find the last input in the table
    const lastInput = document.querySelector('.table-input:last-of-type');
    
    if (lastInput) {
      lastInput.focus(); // Focus the last input field
      simulateTyping(lastInput); // Start typing in the last input field
    }
  }
}

// Continuously check focus every second
setInterval(checkFocusAndAutotype, 1000); // Check every 1 second
