//! Port of `cCPUStack` (avida-core/source/cpu/cCPUStack.{h,cc}).

const STACK_SIZE: usize = 10;

/// Fixed-size circular stack of 10 integers.
#[derive(Debug, Clone, Copy)]
pub struct CpuStack {
    stack: [i32; STACK_SIZE],
    stack_pointer: u8,
}

impl Default for CpuStack {
    fn default() -> Self {
        CpuStack {
            stack: [0; STACK_SIZE],
            stack_pointer: 0,
        }
    }
}

impl CpuStack {
    pub fn push(&mut self, value: i32) {
        if self.stack_pointer == 0 {
            self.stack_pointer = (STACK_SIZE - 1) as u8;
        } else {
            self.stack_pointer -= 1;
        }
        self.stack[self.stack_pointer as usize] = value;
    }

    pub fn pop(&mut self) -> i32 {
        let value = self.stack[self.stack_pointer as usize];
        self.stack[self.stack_pointer as usize] = 0;
        self.stack_pointer += 1;
        if self.stack_pointer as usize == STACK_SIZE {
            self.stack_pointer = 0;
        }
        value
    }

    pub fn peek(&self) -> i32 {
        self.stack[self.stack_pointer as usize]
    }

    pub fn get(&self, depth: i32) -> i32 {
        let mut array_pos = depth as usize + self.stack_pointer as usize;
        if array_pos >= STACK_SIZE {
            array_pos -= STACK_SIZE;
        }
        self.stack[array_pos]
    }

    pub fn top(&self) -> i32 {
        self.stack[self.stack_pointer as usize]
    }

    pub fn clear(&mut self) {
        self.stack = [0; STACK_SIZE];
        self.stack_pointer = 0;
    }

    pub fn flip(&mut self) {
        let mut temp = [0i32; STACK_SIZE];
        for item in temp.iter_mut() {
            *item = self.pop();
        }
        for &item in temp.iter() {
            self.push(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_wraps_at_ten() {
        let mut s = CpuStack::default();
        for i in 1..=12 {
            s.push(i);
        }
        assert_eq!(s.pop(), 12);
        assert_eq!(s.get(0), 11);
        assert_eq!(s.get(8), 3);
        // The popped slot was zeroed and is now the deepest entry.
        assert_eq!(s.get(9), 0);
    }

    #[test]
    fn flip_reverses() {
        let mut s = CpuStack::default();
        s.push(1);
        s.push(2);
        s.flip();
        assert_eq!(s.get(9), 2);
        assert_eq!(s.get(8), 1);
        assert_eq!(s.top(), 0);
    }
}
