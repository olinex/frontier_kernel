.altmacro
.set WORD_SIZE, 8
.macro SAVE_CALLEE_SAVE_REGISTER n
    sd s\n, (\n+2)*WORD_SIZE(a0)
.endm
.macro LOAD_CALLEE_SAVE_REGISTER n
    ld s\n, (\n+2)*WORD_SIZE(a1)
.endm
    .section .text
    .globl _fn_switch_task
    .align 4
_fn_switch_task:
    # stage [1]
    # __switch(
    #     current_task_cx_ptr: *mut TaskContext,
    #     next_task_cx_ptr: *const TaskContext
    # )
    # stage [2]
    # save kernel stack of current task
    sd sp, WORD_SIZE(a0)
    # save ra & s0~s11 of current execution
    sd ra, 0(a0)
    .set n, 0
    .rept 12
        SAVE_CALLEE_SAVE_REGISTER %n
        .set n, n + 1
    .endr
    # stage [3]
    # restore ra & s0~s11 of next execution
    ld ra, 0(a1)
    # restore kernel stack of next task
    ld sp, WORD_SIZE(a1)
    .set n, 0
    .rept 12
        LOAD_CALLEE_SAVE_REGISTER %n
        .set n, n + 1
    .endr
    # stage [4]
    ret