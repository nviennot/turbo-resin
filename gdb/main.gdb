# Set backtrace limit to not have infinite backtrace loops
set backtrace limit 32
set pagination off

# Print demangled symbols
set print asm-demangle on

# Break on bad things happening
#break DefaultHandler
#break HardFault
#break rust_begin_unwind

# Print 5 instructions every time we break.
# Note that `layout asm` is also pretty good, but my up arrow doesn't work
# anymore in this mode, so I prefer display/5i.
display/5i $pc

define count_instr_until
  set $count=0
  while ($pc != $arg0)
    stepi
    set $count=$count+1
  end
  print $count
end
