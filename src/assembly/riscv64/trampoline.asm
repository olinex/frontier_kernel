// enable macro mode
.altmacro
.set WORD_SIZE, 8
// declare a macro which will save register n as double words in to memory
.macro SAVE_GENERAL_PURPOSE_REGISTER n
    sd x\n, \n*WORD_SIZE(sp)
.endm
// declare a macro which will load value as double words from memory
.macro LOAD_GENERAL_PURPOSE_REGISTER n
    ld x\n, \n*WORD_SIZE(sp)
.endm
    .section .text.trampoline
    .global _fn_save_all_registers_before_trap
    .global _fn_restore_all_registers_after_trap
// make align for function pointer address which was strict by riscv
    .align 4
// define a function which will save all general purpose register/sstatus/sepc to kernel memory stack
_fn_save_all_registers_before_trap:
    // which will save csr value to rd and save rs value to csr
    // this instruction in here will swap the value of the sp and sscratch
    // now sp->kernel stack, sscratch->user stack
    csrrw sp, sscratch, sp
    // mallocate a src/trap/TrapContext on kernel stack
    // because the context has 32 registers, sstatus and sepc propertiries
    // in different targets, it maybe 32 bits or 64 bits size
    // so we choice bigger size 64 (memory size unit is byte)
    addi sp, sp, -34*WORD_SIZE
    // save general purpose registers
    // skip zero(x0), because it' value is always zero
    sd ra, 1*WORD_SIZE(sp)
    // skip sp(x2), we will save it later
    sd gp, 3*WORD_SIZE(sp)
    // save x4~x31
    .set n, 4
    .rept 28
        SAVE_GENERAL_PURPOSE_REGISTER %n
        .set n, n+1
    .endr
    // we can use t0(x5)/t1(x6)/t2(x7) freely, because they were saved on kernel stack
    // read sstatus register's value to t0 and save it to stack
    csrr t0, sstatus
    sd t0, 32*WORD_SIZE(sp)
    // read sepc register's value to t1 and save it to stack
    csrr t1, sepc
    sd t1, 33*WORD_SIZE(sp)
    // read user stack from sscratch and save it on the kernel stack
    // because before trap back to user mode, we must know where to jump
    csrr t2, sscratch
    sd t2, 2*WORD_SIZE(sp)
    // load kernel satp into t0
    ld t0, 34*WORD_SIZE(sp)
    // load trap handler virtual memory address into t1
    ld t1, 35*WORD_SIZE(sp)
    // load kernel stack pointer into sp
    ld sp, 36*WORD_SIZE(sp)
    // switch to kernel space
    csrw satp, t0
    // refersh tlb
    sfence.vma
    // jump to trap_handler
    // we cannot use `call trap_handler` here
    // because when we use `call`, the virtual address will be calculated with the pc and diff
    jr t1
    // if everything is fine trap_handler will return and run next instruction
// define a function that will restore all general purpose register/sstatus/sepc values from memory
_fn_restore_all_registers_after_trap:
    // a0: *TrapContext in user space(Constant)
    // a1: user space mmu token
    // switch to user space
    csrw satp, a1
    // refresh tlb
    sfence.vma
    // keep origin user's *TrapContext stack pointer
    csrw sscratch, a0
    // switch stack to user's *TrapContext
    mv sp, a0
    // read sstatus value from memory and save it to register
    ld t0, 32*WORD_SIZE(sp)
    csrw sstatus, t0
    // read sepc value from memory and save it to register
    ld t1, 33*WORD_SIZE(sp)
    csrw sepc, t1
    // restore general purpuse registers except zero/sp
    ld ra, 1*WORD_SIZE(sp)
    ld gp, 3*WORD_SIZE(sp)
    .set n, 4
    .rept 28
        LOAD_GENERAL_PURPOSE_REGISTER %n
        .set n, n+1
    .endr
    // back to user stack
    ld sp, 2*WORD_SIZE(sp)
    sret
