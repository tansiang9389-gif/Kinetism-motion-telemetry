const connection = document.querySelector("#connection");
const motion = document.querySelector("#motion");
const elbowAngle = document.querySelector("#elbowAngle");
const forceNewton = document.querySelector("#forceNewton");
const maxFlexion = document.querySelector("#maxFlexion");
const rangeOfMotion = document.querySelector("#rangeOfMotion");
const forearm = document.querySelector("#forearm");
const wrist = document.querySelector("#wrist");
const angleArc = document.querySelector("#angleArc");
const angleLabel = document.querySelector("#angleLabel");
const imuMarker = document.querySelector("#imuMarker");
const metricMotion = document.querySelector(".metric.accent");
const canvas = document.querySelector("#chart");
const context = canvas.getContext("2d");
const history = [];
const armMassInput = document.querySelector("#armMass");
const forceThresholdInput = document.querySelector("#forceThreshold");
let socket;
let lastPacketAt = 0;
let filteredAngle = 0;
let sessionMinimum = 0;
let sessionMaximum = 0;
let calibrated = false;
let displayedForceN = 0;

armMassInput.value = localStorage.getItem("kinetismArmMassKg") || "1.5";
forceThresholdInput.value = localStorage.getItem("kinetismForceThresholdN") || "1.0";

function numericSetting(input, fallback) {
  const value = Number.parseFloat(input.value);
  return Number.isFinite(value) && value > 0 ? value : fallback;
}

function saveForceSettings() {
  localStorage.setItem("kinetismArmMassKg", String(numericSetting(armMassInput, 1.5)));
  localStorage.setItem(
    "kinetismForceThresholdN",
    String(numericSetting(forceThresholdInput, 1.0))
  );
  displayedForceN = 0;
  forceNewton.textContent = "0.0";
}

armMassInput.addEventListener("change", saveForceSettings);
forceThresholdInput.addEventListener("change", saveForceSettings);

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
  document.querySelector("#battery").textContent = `${data.battery_percent}%`;
  document.querySelector("#sampleRate").textContent = `${data.sample_rate_hz.toFixed(1)} Hz`;
  document.querySelector("#packets").textContent = data.packets_received;
  document.querySelector("#lost").textContent = data.packets_lost;
  document.querySelector("#accuracy").textContent = `${data.quaternion_accuracy}/3`;
  document.querySelector("#intensity").textContent = `${data.magnitude_g.toFixed(3)} g`;
  updateForce(data);

  const [w, x, y, z] = data.relative_quaternion;
  const forearmX = Math.max(-1, Math.min(1, 1 - 2 * (y * y + z * z)));
  const rawAngle = Math.acos(forearmX) * 180 / Math.PI;
  filteredAngle += 0.2 * (rawAngle - filteredAngle);
  filteredAngle = Math.max(0, Math.min(160, filteredAngle));

  if (!calibrated) {
    sessionMinimum = filteredAngle;
    sessionMaximum = filteredAngle;
    calibrated = true;
  } else {
    sessionMinimum = Math.min(sessionMinimum, filteredAngle);
    sessionMaximum = Math.max(sessionMaximum, filteredAngle);
  }

  function updateForce(data) {
    const massKg = numericSetting(armMassInput, 1.5);
    const refreshThresholdN = numericSetting(forceThresholdInput, 1.0);
    const estimatedForceN = massKg * data.magnitude_g * 9.80665;

    if (
      data.moving &&
      estimatedForceN >= displayedForceN + refreshThresholdN
    ) {
      displayedForceN = estimatedForceN;
    }
    forceNewton.textContent = displayedForceN.toFixed(1);
  }

  elbowAngle.textContent = filteredAngle.toFixed(1);
  maxFlexion.textContent = sessionMaximum.toFixed(1);
  rangeOfMotion.textContent = (sessionMaximum - sessionMinimum).toFixed(1);
  updateArmModel(filteredAngle);

  history.push(filteredAngle);
  while (history.length > 1000) history.shift();
  drawChart();
}

function updateArmModel(angleDegrees) {
  const elbowX = 198;
  const elbowY = 205;
  const length = 132;
  const angle = angleDegrees * Math.PI / 180;
  const wristX = elbowX + length * Math.sin(angle);
  const wristY = elbowY + length * Math.cos(angle);
  forearm.setAttribute("x2", wristX);
  forearm.setAttribute("y2", wristY);
  wrist.setAttribute("cx", wristX);
  wrist.setAttribute("cy", wristY);

  const midpointX = (elbowX + wristX) / 2;
  const midpointY = (elbowY + wristY) / 2;
  imuMarker.setAttribute(
    "transform",
    `translate(${midpointX} ${midpointY}) rotate(${-angleDegrees})`
  );

  const radius = 45;
  const arcStartX = elbowX;
  const arcStartY = elbowY + radius;
  const arcEndX = elbowX + radius * Math.sin(angle);
  const arcEndY = elbowY + radius * Math.cos(angle);
  angleArc.setAttribute(
    "d",
    `M ${arcStartX} ${arcStartY} A ${radius} ${radius} 0 0 0 ${arcEndX} ${arcEndY}`
  );
  angleLabel.setAttribute("x", elbowX + 58);
  angleLabel.setAttribute("y", elbowY + 34);
  angleLabel.textContent = `${angleDegrees.toFixed(1)}°`;
}

function drawChart() {
  const width = canvas.width;
  const height = canvas.height;
  context.clearRect(0, 0, width, height);
  context.fillStyle = "#8190a0";
  context.font = "20px system-ui";
  context.textAlign = "right";
  context.textBaseline = "middle";
  context.strokeStyle = "#26313d";
  context.lineWidth = 1;
  for (let degrees = 0; degrees <= 160; degrees += 40) {
    const y = height - 24 - degrees * (height - 48) / 160;
    context.beginPath();
    context.moveTo(52, y);
    context.lineTo(width, y);
    context.stroke();
    context.fillText(`${degrees}°`, 45, y);
  }
  context.strokeStyle = "#4ce0ad";
  context.lineWidth = 4;
  context.lineJoin = "round";
  context.beginPath();
  history.forEach((angle, index) => {
    const x = 52 + index * (width - 52) / 999;
    const y = height - 24 - angle * (height - 48) / 160;
    if (index === 0) context.moveTo(x, y);
    else context.lineTo(x, y);
  });
  context.stroke();
}

document.querySelector("#zeroButton").addEventListener("click", () => {
  if (socket?.readyState === WebSocket.OPEN) {
    socket.send(JSON.stringify({action: "zero_orientation"}));
    filteredAngle = 0;
    sessionMinimum = 0;
    sessionMaximum = 0;
    history.length = 0;
    calibrated = false;
    displayedForceN = 0;
    forceNewton.textContent = "0.0";
  }
});

document.querySelector("#resetForceButton").addEventListener("click", () => {
  displayedForceN = 0;
  forceNewton.textContent = "0.0";
});

setInterval(() => {
  if (Date.now() - lastPacketAt > 1000) {
    connection.textContent = "Waiting for sensor";
    connection.className = "status offline";
  }
}, 250);

connect();
