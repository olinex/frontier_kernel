    // define a sub section call '.text.entry'
    // which will be the most beginning before other code on .text section
    .section .text.entry
    // declare a glabal symbol '_start'
    // which will be accessible by other object file
    .global _start
// define the symbol '_start'
// which point to the next line of itself
_start:
    // pointed as '_start' and load immediate value
    li x1, 100
#     la sp, boot_stack_top_bound
#     la fp, boot_stack_low_bound
#     call main

#     // deine a sub section name call '.data.bootstack'
#     // which will be the most ending after other code on .data section
#     .section .data.bootstack
#     // declare a global symbol 'boot_stack_low_bound'
#     .global boot_stack_low_bound
# boot_stack_low_bound:
#     // malloc 64 KiB space as boot stack
#     .space 1024 * 64
#     // declare a global symbol 'boot_stack_top_bound'
#     .global boot_stack_top_bound
# boot_stack_top_bound:
