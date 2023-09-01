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
    .section .text
    .global _fn_save_all_registers_before_trap
    .global _fn_restore_all_registers_after_trap
// make align for function pointer address which was strict by riscv
    .align 4
// define a function which will save all general purpose register to kernel memory stack
_fn_save_all_registers_before_trap:
    // csrrw rd, csr, rs
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
    // set input argument of trap_handler(cx: &mut TrapContext)
    // trap_handler require TrapContext as argument
    // we have mallocated and inited it by hand
    // so we bass the stack top pointer to the handler
    mv a0, sp
    // if everything is fine trap_handler will return and run next instruction
    call trap_handler
// define a function that will restore all register values from memory
// this function will be called by two cases:
// case1: back to U after `call trap_handler`, in this case the next instrction, `mv sp, a0`, is useless
// case2: back to U after at first 
_fn_restore_all_registers_after_trap:
    mv sp, a0
    // read sstatus value from memory and save it to register
    ld t0, 32*WORD_SIZE(sp)
    csrw sstatus, t0
    // read sepc value from memory and save it to register
    ld t1, 33*WORD_SIZE(sp)
    csrw sepc, t1
    // read sscratch value from memory and save it to register
    ld t2, 2*WORD_SIZE(sp)
    csrw sscratch, t2
    // restore general purpuse registers except zero/sp
    ld ra, 1*WORD_SIZE(sp)
    ld gp, 3*WORD_SIZE(sp)
    .set n, 4
    .rept 28
        LOAD_GENERAL_PURPOSE_REGISTER %n
        .set n, n+1
    .endr
    // release TrapContext on kernel stack
    addi sp, sp, 34*WORD_SIZE
    # now sp->kernel stack, sscratch->user stack
    csrrw sp, sscratch, sp
    sret
