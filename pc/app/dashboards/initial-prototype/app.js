const connection = document.querySelector("#connection");
const motion = document.querySelector("#motion");
const magnitude = document.querySelector("#magnitude");
const currentPeak = document.querySelector("#currentPeak");
const lastPeak = document.querySelector("#lastPeak");
const sensorModel = document.querySelector("#sensorModel");
const metricMotion = document.querySelector(".metric.accent");
const canvas = document.querySelector("#chart");
const context = canvas.getContext("2d");
const history = [];
let socket;
let lastPacketAt = 0;

function connect() {
  const scheme = location.protocol === "https:" ? "wss" : "ws";
  socket = new WebSocket(`${scheme}://${location.host}/ws`);
  socket.onmessage = event => {
    const data = JSON.parse(event.data);
    lastPacketAt = Date.now();
    updateDashboard(data);
  };
  socket.onclose = () => setTimeout(connect, 1000);
}

function updateDashboard(data) {
  connection.textContent = `Live · ${data.source}`;
  connection.className = "status online";
  motion.textContent = data.moving ? "Moving" : "Stationary";
  metricMotion.classList.toggle("moving", data.moving);
  magnitude.textContent = data.magnitude_g.toFixed(3);
  currentPeak.textContent = data.current_peak_g.toFixed(3);
  lastPeak.textContent = data.last_peak_g.toFixed(3);
  document.querySelector("#battery").textContent = `${data.battery_percent}%`;
  document.querySelector("#sampleRate").textContent = `${data.sample_rate_hz.toFixed(1)} Hz`;
  document.querySelector("#packets").textContent = data.packets_received;
  document.querySelector("#lost").textContent = data.packets_lost;
  document.querySelector("#accuracy").textContent = `${data.quaternion_accuracy}/3`;
  document.querySelector("#noise").textContent = `${data.noise_floor_g.toFixed(3)} g`;

  const [w, x, y, z] = data.relative_quaternion;
  const ysqr = y * y;
  const roll = Math.atan2(2 * (w * x + y * z), 1 - 2 * (x * x + ysqr));
  const pitch = Math.asin(Math.max(-1, Math.min(1, 2 * (w * y - z * x))));
  const yaw = Math.atan2(2 * (w * z + x * y), 1 - 2 * (ysqr + z * z));
  sensorModel.style.transform = `rotateZ(${yaw}rad) rotateY(${pitch}rad) rotateX(${roll}rad)`;

  history.push([...data.linear_accel, data.magnitude_g * 9.80665]);
  while (history.length > 500) history.shift();
  drawChart();
}

function drawChart() {
  const width = canvas.width;
  const height = canvas.height;
  context.clearRect(0, 0, width, height);
  context.strokeStyle = "#26313d";
  context.lineWidth = 1;
  for (let i = 1; i < 4; i++) {
    const y = height * i / 4;
    context.beginPath();
    context.moveTo(0, y);
    context.lineTo(width, y);
    context.stroke();
  }
  const maxAbs = Math.max(2, ...history.flat().map(Math.abs));
  const colors = ["#ff6e6e", "#5ee092", "#66a6ff", "#f2d36b"];
  colors.forEach((color, axis) => {
    context.strokeStyle = color;
    context.lineWidth = axis === 3 ? 2.5 : 1.5;
    context.beginPath();
    history.forEach((sample, index) => {
      const x = index * width / Math.max(499, history.length - 1);
      const y = height / 2 - sample[axis] * (height * .42 / maxAbs);
      if (index === 0) context.moveTo(x, y);
      else context.lineTo(x, y);
    });
    context.stroke();
  });
}

document.querySelector("#zeroButton").addEventListener("click", () => {
  if (socket?.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify({action: "zero_orientation"}));
  }
});

setInterval(() => {
  if (Date.now() - lastPacketAt > 1000) {
    connection.textContent = "Waiting for sensor";
    connection.className = "status offline";
  }
}, 250);

connect();

