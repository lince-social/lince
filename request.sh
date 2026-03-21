  TOKEN=$(
    curl -s -X POST https://manas.lince.social/api/auth/login \
      -H 'Content-Type: application/json' \
      -d '{"username":"bomboclaat","password":"crazyfrog"}' \
    | jq -r '.token'
  )

  curl -N -H "Authorization: Bearer $TOKEN" \
    https://manas.lince.social/api/sse/view/1
