//! Process management syscalls
use crate::{
    config::PAGE_SIZE, mm::{translated_byte_buffer, MapPermission, VPNRange, VirtAddr}, task::{
        change_program_brk, current_user_token, exit_current_and_run_next,
        suspend_current_and_run_next, TASK_MANAGER,
    }, timer::get_time_us
};

#[repr(C)]
#[derive(Debug)]
pub struct TimeVal {
    pub sec: usize,
    pub usec: usize,
}

/// task exits and submit an exit code
pub fn sys_exit(_exit_code: i32) -> ! {
    trace!("kernel: sys_exit");
    exit_current_and_run_next();
    panic!("Unreachable in sys_exit!");
}

/// current task gives up resources for other tasks
pub fn sys_yield() -> isize {
    trace!("kernel: sys_yield");
    suspend_current_and_run_next();
    0
}

/// YOUR JOB: get time with second and microsecond
/// HINT: You might reimplement it with virtual memory management.
/// HINT: What if [`TimeVal`] is splitted by two pages ?
pub fn sys_get_time(ts: *mut TimeVal, _tz: usize) -> isize {
    trace!("kernel: sys_get_time");
    let us = get_time_us(); //获取us时间
    let token = current_user_token();
    //创建当前时间结构体
    let time = TimeVal {
        sec: us / 1_000_000,
        usec: us % 1_000_000,
    };
    let size = core::mem::size_of::<TimeVal>(); //获得一个结构体的大小用于获得用户空间中相同大小的空间
    let buffers = translated_byte_buffer(token, ts as *const u8, size);
    let mut total_copied = 0;
    for buffer in buffers {
        for i in 0..buffer.len() {
            if total_copied >= size {
                break;
            }
            let src_addr = &time as *const _ as usize + total_copied;
            let byte_value = unsafe { *(src_addr as *const u8) };
            buffer[i] = byte_value;
            total_copied += 1;
        }
    }
    0
}

/// TODO: Finish sys_trace to pass testcases
/// HINT: You might reimplement it with virtual memory management.
pub fn sys_trace(trace_request: usize, id: usize, data: usize) -> isize {
    trace!("kernel: sys_trace");
    match trace_request {
        0 => unsafe {
            let token = current_user_token();
            let buffers = translated_byte_buffer(token, id as *const u8, 1);
            if buffers.is_empty() {
                return -1;
            }
            let value = *(id as *const u8);
            return value as isize;
        },
        1 => {
            let token = current_user_token();
            let mut buffers = translated_byte_buffer(token, id as *const u8, 1);
            if buffers.is_empty() {
                return -1;
            }
            buffers[0][0] = data as u8;
            return 0 as isize;
        }
        2 => {
            return TASK_MANAGER.get_task_syscall_count(id) as isize;
        }
        _ => -1,
    }
}

// YOUR JOB: Implement mmap.
pub fn sys_mmap(start: usize, len: usize, prot: usize) -> isize {
    //没有按页对齐
    if start % PAGE_SIZE != 0 {
        return -1;
    }
    //高位不为0
    if (prot & !0x7) != 0 {
        return -1;
    } else if prot & 0x7 == 0 {
        //无意义内存
        return -1;
    }

    //设置内存属性
    let mut permission = MapPermission::U;
    if (prot & 1) != 0 {
        permission |= MapPermission::R;
    } //可读
    if (prot & 2) != 0 {
        permission |= MapPermission::W;
    } //可写
    if (prot & 4) != 0 {
        permission |= MapPermission::X;
    } //可执行

    //获取虚拟地址起止位置
    let start_vpn = VirtAddr::from(start).floor();
    let end_vpn = VirtAddr::from(start+len).ceil();

    //检查vpn是否已经非配
    let vpns = VPNRange::new(start_vpn, end_vpn);
    for vpn in vpns {
        if let Some(pte)  = TASK_MANAGER.get_page_table(vpn) {
            if pte.is_valid() {
                return -1;
            }
        }
    }
    
    //转换为虚拟地址
    let start_va = start.into();
    let end_va = end_vpn.into();

    TASK_MANAGER.create_new_map_area(start_va, end_va, permission);
    0
}

// YOUR JOB: Implement munmap.
pub fn sys_munmap(_start: usize, _len: usize) -> isize {
    trace!("kernel: sys_munmap NOT IMPLEMENTED YET!");
    -1
}
/// change data segment size
pub fn sys_sbrk(size: i32) -> isize {
    trace!("kernel: sys_sbrk");
    if let Some(old_brk) = change_program_brk(size) {
        old_brk as isize
    } else {
        -1
    }
}
