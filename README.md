# Walking on water with this profiler for RISC Zero

<img src="title.png" align="right" alt="A young boy walking on water heading to a place with Bonsai." width="300"/>

This repo presents a plugin for RISC Zero programs that counts the number of cycles contributing by different parts of the program, 
detects execution steps that lead to significant number of cycles, and explains the underlying reasons.

Developers can add `start_timer!`, `stop_start_timer!`, and `stop_timer!` in the program to trace where the cycles come from. An example
is as follows.

```rust
start_timer!("Load data");
......

    start_timer!("Read from the host");
    ......

    stop_start_timer!("Check the length");
    ......

    stop_start_timer!("Hash");
    ......

    stop_timer!();

stop_timer!();
```

The profiler will output colorized information about the breakdown of the cycles. Specifically, if the profiler sees a single execution step 
that, however, leads to a large number of cycles, it would call it out and find out the underlying reasons. 

![An example output of the profiler.](profiler-example.png)

### How to use

There are necessary changes that need to be 

### How does it work?

### License

