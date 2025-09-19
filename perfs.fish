for i in (seq 1 3 10)
     perf record -o (echo $i)_tmpdir_perf.data target/release/server --addr "127.0.0.1:5622" --exit-code ex --dbfile /tmp/(whoami)_(echo $i).db &
     sleep 1
     target/release/benchmark --addr "127.0.0.1:5622" --exit-code ex --threads $i --ops 10000
end
