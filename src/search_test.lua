wrk.method = "POST"
wrk.body = '{"video_id":"load-test-' .. math.random(1000) .. '","hash":"' .. string.rep("0", 64) .. '"}'
wrk.headers["Content-Type"] = "application/json"