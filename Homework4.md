In this fourth assignment, we are starting with the same code as in hw3, but instead of analyzing what's wrong, we implement some fixes. 

They key problem we observed in hw3 was that the server keeps saving the database every time there is an update. 
Program crash persistence is a reasonable requirement for a database. Sometimes, persistence through kernel crash or power failure are also required, but this is beyond the scope of this assignment. 

## 1. Batch-wise persistence

Since the socket protocol of our key-value store supports batching requests, we may reasonably relax the persistence requirement from _every request_ to _every batch_. After all, the client won't know the difference since we are only sending the responses once the entire batch is completed. 

Change the server to save the database to disk only once per batch, and only if there were any changes. 

________________________________________________________
*For reference*: (batch size 100)
`target/release/benchmark --addr "127.0.0.1:5622" --exit-code ex --ops 5000 --threads 10`
`time target/release/server --addr "127.0.0.1:5622" --exit-code ex --dbfile /tmp/(whoami).db`

--- Benchmark Results ---
Total Operations: 50000
Total Duration:   32.8739 s
Avg Operations/s: 1520.97
Throughput:       0.0015 M req/s

Executed in   33.33 secs    fish           external
   usr time   17.57 secs    1.03 millis   17.57 secs
   sys time   15.25 secs    0.50 millis   15.25 secs
________________________________________________________

- [ ] Try running this version with different batch sizes configured in the benchmark client, and see how this impacts performance. 

    - Decreasing batch size leads to lower performance
    - Increasing batch size leads to better performance

________________________________________________________
*Against Reference* (batch size 100)
`target/release/benchmark --addr "127.0.0.1:5622" --exit-code ex --ops 5000 --threads 10`
`time target/release/server --addr "127.0.0.1:5622" --exit-code ex --dbfile /tmp/(whoami).db`

--- Benchmark Results ---
Total Operations: 50000
Total Duration:   0.1107 s
Avg Operations/s: 451689.91
Throughput:       0.4517 M req/s

Executed in  573.58 millis    fish           external
   usr time   57.18 millis  789.00 micros   56.39 millis
   sys time   42.28 millis  385.00 micros   41.90 millis

________________________________________________________
*Batch size: 20*

--- Benchmark Results ---
Total Operations: 50000
Total Duration:   0.1553 s
Avg Operations/s: 321923.56
Throughput:       0.3219 M req/s

Executed in  537.94 millis    fish           external
   usr time   74.66 millis  748.00 micros   73.91 millis
   sys time   51.23 millis  379.00 micros   50.85 millis
________________________________________________________
*Batch size: 300*

--- Benchmark Results ---
Total Operations: 50000
Total Duration:   0.1010 s
Avg Operations/s: 495024.89
Throughput:       0.4950 M req/s

Executed in  507.07 millis    fish           external
   usr time   63.67 millis  700.00 micros   62.97 millis
   sys time   23.66 millis  348.00 micros   23.31 millis
________________________________________________________
*Batch size: 1000*

--- Benchmark Results ---
Total Operations: 50000
Total Duration:   0.0926 s
Avg Operations/s: 540050.66
Throughput:       0.5401 M req/s

Executed in  414.76 millis    fish           external
   usr time   47.25 millis  708.00 micros   46.54 millis
   sys time   38.19 millis  365.00 micros   37.82 millis

________________________________________________________

## 2. Add Write-Ahead Logging

Performance is still quite poor, however, and very large batches may not be realistic in many real applications. To do better, we can implement write-ahead logging: instead of frequently writing out the entire database to disk, simply append all the (successful) original requests to a text file instead. 

On server startup, the template code already first re-does all the requests in the log, before serving new clients. 

- [x] on each update (set / remove), append the request to a log file (use the file name specified in the ``--logfile`` argument). Then process the request as usual, but don't save the table to disk. 
- [x] how does this impact the number of write system calls? What about the duration of each write system call?
    - The number of write calls compared to hw3 is the same (1 write call per mutable operation), but the size of the buffer is much smaller and the duration of that call is much smaller
- [x] for even better performance, write the log to disk only once per batch*. 
    - I think this only works because it's single threaded
- [ ] many database systems and file servers use a separate, faster device for logging. Try putting the log on our fast "device" ``/tmp/``. 

** Note: this all works nicely for application crashes. In the event of power failure, updates may still be lost. To prevent that, you'll need to use the ``fsync`` system call, or close the file. These are both fairly expensive, and out of scope for this assignment. **

## 3. Avoid long startup times with periodic snapshots

Keeping all updates in a log will eventually lead to extremely long startup times. Consider a database that has been serving a million requests per second, for the past day.... 

A better solution is to use write-ahead logging until the log exceeds a certain length, then write out the full database to disk, and zero out the log. 
To zero out an open log file, use both Seek::rewind(), and File::set_len(0) together. 

- [ ] Add a snapshot interval command line argument to the server.
  - This argument **must be** passable as `--snapshot-interval <n>`, for `<n>` some positive integer
    - If it is *not* passable as such, the autograding script **will fail and give you a 0**
- [ ] Observe the throughput achieved for snapshot intervals ranging from 10 requests to 10000 requests
- [ ] Observe the mean and tail latency for snapshot intervals ranging from 10 requests to 10000 requests
-  ] What do you notice about the relationship between mean latency and throughput? 
- [ ] Is there a similar relationship between tail latency and throughput?
- [ ] Measure mean/tail latency over batch size. 

