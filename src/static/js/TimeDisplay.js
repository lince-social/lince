import React, {useState, useEffect} from 'react'

function TimeDisplay({ initialTime }) {
  const [currentTime, setCurrentTime] = useState(initialTime);

  useEffect(() => {
    const intervalId = setInterval(() => {
      fetch('/current_time')
        .then(response => response.json())
        .then(data => setCurrentTime(data.current_time));
    }, 1000);
    return () => clearInterval(intervalId);
  }, []);
  return (
    <p style={{ marginLeft: 'auto' }}>{currentTime}</p>
  );
}

export default TimeDisplay;
