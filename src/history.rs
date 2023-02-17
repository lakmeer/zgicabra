
use std::fmt;

pub struct History<T> {
    buffer: Vec<Option<T>>,
    last: usize,
    tail: usize,
    size: usize,
}

impl<T: fmt::Debug + Clone> fmt::Debug for History<T> {

    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Hist[ ")?;

        for ix in 0..self.size {
            let item = self.get(ix);

            match item {
                Some(item) => write!(f, "{:?}", item)?,
                None       => write!(f, "_")?,
            }

            if ix < self.size - 1 {
                write!(f, ", ")?;
            }
        }

        write!(f, " ]")
    }

}

impl<T:Clone> History<T> {
    pub fn new(size: usize) -> Self {
        Self {
            buffer: vec![None; size],
            last: 0,
            tail: 0,
            size: size,
        }
    }

    pub fn push(&mut self, item: T) {
        self.buffer[self.tail] = Some(item);
        self.last = self.tail;
        self.tail = (self.tail + 1) % self.size;
        if self.tail == self.last {
            self.last = (self.last + 1) % self.size;
        }
    }

    pub fn get(&self, index: usize) -> Option<&T> {
        self.buffer[(self.tail + index) % self.size].as_ref()
    }

    pub fn get_from_end(&self, index: usize) -> Option<&T> {
        let unindex = self.size + self.last - index;
        self.buffer[unindex % self.size].as_ref()
    }

    pub fn last(&self) -> Option<&T> {
        self.buffer[self.last].as_ref()
    }

    pub fn size(&self) -> usize {
        self.size
    }
}


