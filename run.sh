#!/bin/bash
cd /home/const/wazzup 

pkill -f "target/debug/main" 2>/dev/null
pkill -f "cargo run" 2>/dev/null

RUST_LOG=info nohup cargo run --bin main > server.log 2>&1 &