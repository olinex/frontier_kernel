    // define a sub section call '.text.entry'
    // which will be the most beginning before other code on .text section
    .section .text.entry
    // declare a glabal symbol '_fn_start'
    // which will be accessible by other object file
    .global _fn_start
// define the symbol '_fn_start'
// which point to the next line of itself
_fn_start:
    la sp, _addr_bootstack_bigger_bound
    // the main fuction is the entry of the system
    // no more other instrctions will be executed
    // so we no need to use call because main function will not return
    j main

    // deine a sub section name call '.data.bootstack'
    // which will be the most ending after other code on .data section
    .section .bss.bootstack
    // declare a global symbol '_addr_bootstack_smaller_bound'
    .global _addr_bootstack_smaller_bound
_addr_bootstack_smaller_bound:
    // malloc 64 KiB space as boot stack
    .space 1024 * 64
    // declare a global symbol '_addr_bootstack_bigger_bound'
    .global _addr_bootstack_bigger_bound
_addr_bootstack_bigger_bound:
