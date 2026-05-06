/* Template for generated amd64 forwarding veneers.
 *
 * build.rs expands this shape for ABI exports not provided by the Rust object
 * set. Each generated veneer preserves the incoming ABI registers, resolves
 * the exact symbol version from the private backend with dlvsym, then tail
 * jumps to that backend implementation.
 */

.text
.globl __safelibs_forwarding_veneer_template
.type __safelibs_forwarding_veneer_template, @function
__safelibs_forwarding_veneer_template:
    push %rax
    push %rdi
    push %rsi
    push %rdx
    push %rcx
    push %r8
    push %r9
    leaq .Lsafelibs_template_symbol(%rip), %rdi
    leaq .Lsafelibs_template_version(%rip), %rsi
    call __safelibs_resolve_versioned_symbol@PLT
    mov %rax, %r11
    pop %r9
    pop %r8
    pop %rcx
    pop %rdx
    pop %rsi
    pop %rdi
    pop %rax
    jmp *%r11
.size __safelibs_forwarding_veneer_template, .-__safelibs_forwarding_veneer_template

.pushsection .rodata
.Lsafelibs_template_symbol:
    .asciz "__safelibs_template_symbol"
.Lsafelibs_template_version:
    .asciz "GLIBC_PRIVATE"
.popsection
