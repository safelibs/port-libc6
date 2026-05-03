Session Goal:
port the libc6 library from C to Rust

Recent Successful Session History Summaries:
- None.

Local Planning Request:
Thoroughly analyze the libc6 code in /home/yans/safelibs/pipeline/ports/port-libc6/original to
        develop a plan to port the libc6 library from C to Rust into
        /home/yans/safelibs/pipeline/ports/port-libc6/safe. The library should be:

        - **source-compatible**, so a C program that uses libc6 should
          be able to compile against libc6-safe,
          meaning that all public APIs should remain exported and compatible. All test cases in /home/yans/safelibs/pipeline/ports/port-libc6/original
          should continue to pass. Programs in /home/yans/safelibs/pipeline/ports/port-libc6/dependents.json (as harnessed in /home/yans/safelibs/pipeline/ports/port-libc6/test-original.sh)
          should continue to compile.
        - **link-compatible**, so an object file previously compiled
          against the original libc6 should be able to
          link against libc6-safe, meaning all symbols should be identically exported. Test file objects from
          /home/yans/safelibs/pipeline/ports/port-libc6/original should continue to link against libc6-safe and run properly.
        - **runtime-compatible**, so a program that relies on the original
          libc6 should run perfectly when the library is replaced with
          libc6-safe. Programs in /home/yans/safelibs/pipeline/ports/port-libc6/dependents.json (as harnessed in /home/yans/safelibs/pipeline/ports/port-libc6/test-original.sh)
          should continue to function with libc6-safe just as they did with the original libc6.
        - **reasonably safe**: unsafe Rust is okay as an intermediate step,
          but all code in the final result should be safe
          unless it MUST be unsafe (e.g., to interface with C application code or the OS).
        - **drop-in replaceable**: libc6-safe should ship as a package
          for ubuntu 24.04. /home/yans/safelibs/pipeline/ports/port-libc6/test-original.sh and related files should
          be modified to install the libc6-safe package and ensure continued functionality of all
          software described in /home/yans/safelibs/pipeline/ports/port-libc6/dependents.json.

        Priorities, from most to least (but still) important, are:

        1. perfect compile and runtime interoperability. This is a must-have.
        2. security, both memory safety and resilience against
           previously-identified non-memory vulnerabilities such as
           all those in /home/yans/safelibs/pipeline/ports/port-libc6/relevant_cves.json, which must be mitigated in libc6-safe.
        3. performance. Good to have, but not at the expense of the other two.

        The library should be contained in /home/yans/safelibs/pipeline/ports/port-libc6/safe as a standard Rust package.
        For testing, the test cases in /home/yans/safelibs/pipeline/ports/port-libc6/original must be ported over.

        Each implementation phase should commit to git so that the
        succeeding checkers can reason about what was changed.
        Ensure that this workflow is linear: checkers must only bounce
        back to the previous implementor. This means that each major
        testing step (e.g., each class of test cases) will probably
        require its own implementation phase followed by checking phase,
        and will likely require a general "fix everything remaining" sort
        of catch-all implementation phase toward the end. Make sure all
        the test cases for all the above properties are thoroughly
        checked at the end.
