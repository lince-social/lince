
#!/usr/bin/env bash
set -euo pipefail

port="${PORT:-6175}"
dir="$(mktemp -d)"
html="$dir/index.html"

cat > "$html" <<'HTML'
<!doctype html>
<html>
<head>
  <meta charset="utf-8">
  <title>Local Leaflet Map</title>
  <link rel="stylesheet" href="https://unpkg.com/leaflet@1.9.4/dist/leaflet.css">
  <style>
    html, body, #map { height: 100%; margin: 0; }
  </style>
</head>
<body>
<div id="map"></div>

<script src="https://unpkg.com/leaflet@1.9.4/dist/leaflet.js"></script>
<script>
const points = [
  {
    text: "Example by coordinates",
    qty: 1000,
    lat: -32.184593,
    lon: -52.159202
  },
  {
    text: "Example by address",
    qty: 500,
    address: "Avenida Rio Grande, Balneário Cassino, Rio Grande, Rio Grande do Sul, South Region, 96207-490, Brazil"
  },
  ]

const map = L.map("map").setView([-30.0346, -51.2177], 13)

L.tileLayer("https://tile.openstreetmap.org/{z}/{x}/{y}.png", {
  attribution: "© OpenStreetMap contributors"
}).addTo(map)

async function geocode(address) {
  const url = "https://nominatim.openstreetmap.org/search?format=json&limit=1&q=" +
    encodeURIComponent(address)

  const res = await fetch(url, {
    headers: { "Accept": "application/json" }
  })

  const data = await res.json()
  if (!data[0]) throw new Error("Address not found: " + address)

  return {
    lat: Number(data[0].lat),
    lon: Number(data[0].lon)
  }
}

async function main() {
  const bounds = []

  for (const p of points) {
    let pos

    if (p.lat != null && p.lon != null) {
      pos = { lat: p.lat, lon: p.lon }
    } else {
      pos = await geocode(p.address)
      await new Promise(r => setTimeout(r, 1100))
    }

    const radiusMeters = p.qty

    L.circle([pos.lat, pos.lon], {
      radius: radiusMeters,
      weight: 2,
      fillOpacity: 0.35
    }).addTo(map).bindPopup(`${p.text}<br>${p.qty}m radius`)

    L.marker([pos.lat, pos.lon])
      .addTo(map)
      .bindTooltip(p.text, { permanent: true, direction: "center", className: "label" })

    bounds.push([pos.lat, pos.lon])
  }

  if (bounds.length) map.fitBounds(bounds, { padding: [50, 50] })
}

main().catch(err => alert(err.message))
</script>
</body>
</html>
HTML

xdg-open "http://127.0.0.1:$port" >/dev/null 2>&1 &
python3 -m http.server "$port" --directory "$dir"
