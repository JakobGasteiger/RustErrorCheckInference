

struct Log {
    content: String,
}

impl Log {

    fn log(self: &mut Self, info: String) {
        self.content = self.content.clone() + "\n" + &info;
    }

    fn output(self: &Self) {
        println!("{}", self.content);
    }
}