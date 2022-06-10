Reprosched
==========

Scheduler benchmark framework, written in Rust.

Currently implements two tests simulating two important workloads
for Meta production: web and xdb.

Tests are in src/tests. Look at the web test for the example on
how to write tests.

Build/run in the --release mode, otherwise it will take too long
for tests to run.

Results might be flaky if the test is runned on a laptop due
to the cpu frequency scaling and power throttling.

Tests are running until a stable result will be obtained or
the number of attempts will exceed 10.

E.g.:
```
    n  1    1519353
    n  2    1580586 (± 16.4%)
    --------------- discard ----------------
    n  1    1505764
    n  2    1514584 (± 2.5%)
    n  3    1544058 (± 10.1%)
    --------------- discard ----------------
    n  1    1617176
    n  2    1606153 (± 2.9%)
    n  3    1645072 (± 12.5%)
    --------------- discard ----------------
    n  1    1732681
    n  2    1738164 (± 1.3%)
    n  3    1746761 (± 2.7%)
    n  4    1749062 (± 2.4%)
    n  5    1776681 (± 10.6%)
    --------------- discard ----------------
    n  1    1812608
    n  2    1826930 (± 3.3%)
    --------------- discard ----------------
    n  1    1826546
    n  2    1850692 (± 5.5%)
    --------------- discard ----------------
    n  1    1792598
    n  2    1834142 (± 9.6%)
    --------------- discard ----------------
    n  1    1892144
    n  2    1869416 (± 5.2%)
    --------------- discard ----------------
    n  1    1980341
    n  2    1980884 (± 0.1%)
    ========================================
    Result: 1.980884s ± 2.305ms (± 0.1%)
```
Usage:
1) Run web test:
    ```cargo run --release run web```

2) Run xdb test:
    ```cargo run --release run xdb```

3) Get data to calibrate compute() cost (takes a while):
    ```cargo run --release calibrate```
