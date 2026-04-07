#!/usr/bin/env bash
# Starts the server with a fresh DB and opens a localtunnel for mobile testing.
# Usage: ./tests/tunnel.sh
# Press Ctrl+C to stop both the server and tunnel.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
SERVER_DIR="$PROJECT_DIR/server"
BINARY="$PROJECT_DIR/target/release/spinwin-server"
PORT=3000

SERVER_PID=""
TUNNEL_PID=""

cleanup() {
    echo ""
    echo "Shutting down..."
    [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null
    [ -n "$TUNNEL_PID" ] && kill "$TUNNEL_PID" 2>/dev/null
    [ -n "$SERVER_PID" ] && wait "$SERVER_PID" 2>/dev/null
    [ -n "$TUNNEL_PID" ] && wait "$TUNNEL_PID" 2>/dev/null
    echo "Done."
}
trap cleanup EXIT INT TERM

# Kill anything already on the port
kill $(lsof -t -i:$PORT) 2>/dev/null
sleep 1

# Build
source "$HOME/.cargo/env"
echo "Building release binary..."
cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml" 2>&1 | tail -1

# Start server with fresh DB
cd "$SERVER_DIR"
rm -f spinwin.db
"$BINARY" &
SERVER_PID=$!
sleep 2

if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "Server failed to start"
    exit 1
fi
echo "Server running on port $PORT (PID $SERVER_PID)"
echo ""

# Start tunnel (retry up to 3 times)
RETRIES=3
for i in $(seq 1 $RETRIES); do
    echo "Starting localtunnel (attempt $i/$RETRIES)..."
    npx localtunnel --port $PORT &
    TUNNEL_PID=$!
    sleep 5

    if kill -0 "$TUNNEL_PID" 2>/dev/null; then
        echo ""
        echo "============================================"
        echo "  Tunnel is running!"
        echo ""
        echo "  Main page:  <see URL above>"
        echo "  Scanner:    <URL above>/scan"
        echo ""
        echo "  Press Ctrl+C to stop"
        echo "============================================"
        break
    else
        echo "Tunnel failed, retrying..."
        if [ "$i" -eq "$RETRIES" ]; then
            echo ""
            echo "Localtunnel failed after $RETRIES attempts."
            echo "Falling back to serveo.net..."
            ssh -o StrictHostKeyChecking=no -R 80:localhost:$PORT serveo.net &
            TUNNEL_PID=$!
            sleep 5
            if kill -0 "$TUNNEL_PID" 2>/dev/null; then
                echo ""
                echo "============================================"
                echo "  Serveo tunnel is running!"
                echo "  Scanner:  <URL above>/scan"
                echo "  Press Ctrl+C to stop"
                echo "============================================"
            else
                echo "All tunnel options failed."
                echo "Server is still running at http://localhost:$PORT"
                echo "Try accessing via your local IP:"
                echo "  http://$(hostname -I | awk '{print $1}'):$PORT"
                TUNNEL_PID=$$  # dummy so cleanup doesn't error
            fi
        fi
    fi
done

# Wait for either process to exit
wait "$TUNNEL_PID" "$SERVER_PID"
