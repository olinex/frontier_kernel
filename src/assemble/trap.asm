// enable macro mode
.altmacro
// declare a macro which will save register n as double words in to memory
.macro SAVE_GENERAL_PURPOSE_REGISTER n
    sd x\n, \n*8(sp)
.endm
// declare a macro whill will load value as double words from memory
.macro LOAD_GENERAL_PURPOSE_REGISTER n
    ld x\n, \n*8(sp)
.endm
    .section .text
    .globl _addr_save_all_registers_before_trap
    .globl _addr_restore_all_registers_after_trap
// make align for function pointer address which was strict by riscv
    .align 4
// define a function which will save all general purpose register to memory stack
_addr_save_all_registers_before_trap:
    // csrrw rd, csr, rs
    // which wiil save csr value to rd and save rs value to csr
    // this instruction in here will swap the value of the sp and sscratch
    // now sp->kernel stack, sscratch->user stack
    csrrw sp, sscratch, sp
    // mallocate a src/execenv/TrapContext on kernel stack
    // because the context has 32 registers, sstatus and sepc propertiries
    // in different targets, it maybe 32 bits or 64 bits size
    // so we choice bigger size 64 (memory size unit is byte)
    addi sp, sp, -34*8
    // save general purpose registers
    // skip sp(x0), because it' value is always zero
    sd x1, 1*8(sp)
    // skip sp(x2), we will save it later
    sd x3, 3*8(sp)
    // skip tp(x4), application does not use it
    // save x5~x31
    .set n, 5
    .rept 27
        SAVE_GENERAL_PURPOSE_REGISTER %n
        .set n, n+1
    .endr
    // we can use t0/t1/t2 freely, because they were saved on kernel stack
    // read sstatus register's value to t0 and save it to stack
    csrr t0, sstatus
    sd t0, 32*8(sp)
    // read sepc register's value to t1 and save it to stack
    csrr t1, sepc
    sd t1, 33*8(sp)
    // read user stack from sscratch and save it on the kernel stack
    // because before trap back to user mode, we must know where to jump
    csrr t2, sscratch
    sd t2, 2*8(sp)
    // set input argument of trap_handler(cx: &mut TrapContext)
    // trap_handler require TrapContext as argument
    // we have mallocated and inited it by hand
    // so we bass the stack top pointer to the handler
    mv a0, sp
    call trap_handler
// define a function that will restore all register values from memory
_addr_restore_all_registers_after_trap:
    # case1: start running app by __restore
    # case2: back to U after handling trap
    // now sp->kernel stack(after allocated), sscratch->user stack
    mv sp, a0
    // read sstatus value from memory and save it to register
    ld t0, 32*8(sp)
    csrw sstatus, t0
    // read sepc value from memory and save it to register
    ld t1, 33*8(sp)
    csrw sepc, t1
    // read sscratch value from memory and save it to register
    ld t2, 2*8(sp)
    csrw sscratch, t2
    // restore general purpuse registers except x0/x2/x4
    ld x1, 1*8(sp)
    ld x3, 3*8(sp)
    .set n, 5
    .rept 27
        LOAD_GENERAL_PURPOSE_REGISTER %n
        .set n, n+1
    .endr
    // release TrapContext on kernel stack
    addi sp, sp, 34*8
    # now sp->kernel stack, sscratch->user stack
    csrrw sp, sscratch, sp
    sret
