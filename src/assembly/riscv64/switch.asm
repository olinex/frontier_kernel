.altmacro
.set WORD_SIZE, 8
.macro SAVE_CALLEE_SAVE_REGISTER n
    sd s\n, (\n+2)*WORD_SIZE(a0)
.endm
.macro LOAD_CALLEE_SAVE_REGISTER n
    ld s\n, (\n+2)*WORD_SIZE(a1)
.endm
    .section .text
    .global _fn_switch_task
    .global _fn_run_first_task
    .align 4

# _fn_switch_task(
#     current_task_ctx_ptr: *mut TaskContext,
#     next_task_ctx_ptr: *const TaskContext
# )
# switch the current task to next task
_fn_switch_task:
    # save kernel stack of current task
    sd sp, WORD_SIZE(a0)
    # save ra & s0~s11 of current execution
    sd ra, 0(a0)
    .set n, 0
    .rept 12
        SAVE_CALLEE_SAVE_REGISTER %n
        .set n, n + 1
    .endr
    # restore ra & s0~s11 of next execution
    ld ra, 0(a1)
    .set n, 0
    .rept 12
        LOAD_CALLEE_SAVE_REGISTER %n
        .set n, n + 1
    .endr
    # restore kernel stack of next task
    ld sp, WORD_SIZE(a1)
    # go to the code which ra register is pointing to
    # which is the `trap::handler::trap_return`
    ret